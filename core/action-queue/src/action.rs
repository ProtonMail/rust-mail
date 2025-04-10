use crate::db::{ExecutionGuard, StoredAction};
use crate::queue::{QueuedAction, QueuedMetadata, TypeErasedAction};
use anyhow::Context;
use derive_more::derive::TryFrom;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::sql_using_serde;
use stash::stash::{Bond, RunTransaction, StashError, Tether};
use std::any::Any;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Actions can return any error type, but we need to be able to inspect the http request error
/// to detect network failure and [`WriterGuard`] expirations so we can gracefully retry the
/// action.
pub trait Error: std::error::Error + Send + Sync {
    /// Check if the error is the result of a network failure.
    ///
    /// An error is considered a network failure the server replies with 429/5xx HTTP status codes
    /// or there was an issue with the underlying network transport layer.
    fn is_network_failure(&self) -> bool;

    /// Check whether this error was the result of a [`WriterGuardError::Expired`].
    ///
    /// This should return true when the presence of this error is detected.
    fn is_writer_guard_expired(&self) -> bool;
}

/// Errors that may occur during action version conversion.
#[derive(Debug, thiserror::Error)]
pub enum VersionConverterError {
    #[error("Deserialization error: {0}")]
    Deserialization(#[source] rmp_serde::encode::Error),
    /// Return this error
    #[error("Action version {0} is invalid")]
    InvalidVersion(u32),
    #[error("Failed to migrate action: {0}")]
    Failure(#[source] anyhow::Error),
}

/// Unique identifier for each type of action.
///
/// It is recommended this value be a human-readable string to aid in debugging.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Type(pub &'static str);

impl Type {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
}

impl AsRef<str> for Type {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Unique identifier for each action execution group.
///
/// It is recommended this value be a human-readable string to aid in debugging.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ActionGroup(pub &'static str);

impl ActionGroup {
    /// Create new action group identifier.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Get the default action group.
    #[must_use]
    pub const fn default() -> Self {
        Self("DEFAULT_GROUP")
    }
}

impl AsRef<str> for ActionGroup {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Display for ActionGroup {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// Defines the priority of a queued action.
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum Priority {
    Highest = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

impl ToSql for Priority {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for Priority {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

/// Version conversion for stored actions.
///
/// This will be called if we encounter a queued action which was created with an older
/// version of the implementation.
pub trait VersionConverter {
    /// Output of the conversion.
    type Output: Action;
    /// Convert the serialized action from the `old_version` to the `new_version`.
    ///
    /// The `data` for this action was recorded when the action was at `old_version`.
    ///
    /// # Remarks
    /// This function is also called if `old_version` and `new_version` have the samve value.
    ///
    /// # Errors
    /// Return error if the version conversion failed.
    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output>;
}

/// Default version converter implementation.
///
/// If the versions don't match it will throw an error.
pub struct DefaultVersionConverter<T>(PhantomData<T>);

impl<T: Action> VersionConverter for DefaultVersionConverter<T> {
    type Output = T;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        if old_version == current_version {
            Ok(deserialize::<T>(data)?)
        } else {
            Err(FactoryError::InvalidVersion(current_version))
        }
    }
}

/// Required metadata to define an action.
///
/// An Action is an operation in the system that is applied opportunistically to the local data set
/// first and executed on the remote server as soon as possible.
///
/// Its is recommended to assign this trait to the part of the action which contains the
/// data required for it to operate on. Execution of the action is defined by the [`Handler`] trait.
///
/// Each action can also be assigned an execution context which can be
/// used to pass in runtime data. The context needs to be registered with
/// the queue before it can be used. See
/// [`register_execution_context()`](`queue::Queue::register_execution_context()`)
/// for more details.
///
pub trait Action: Serialize + DeserializeOwned + 'static + Send {
    /// Unique type identifier.
    const TYPE: Type;

    /// The Group in which this action should execute.
    const GROUP: ActionGroup = ActionGroup::default();

    /// Version of the current implementation.
    const VERSION: u32;

    /// Default priority for this action.
    ///
    /// Can be overridden with [`MetadataBuilder::with_priority_override`].
    const PRIORITY: Priority = Priority::Normal;

    /// Associated version converter.
    ///
    /// For more details see the [`VersionConverter`] trait.
    type VersionConverter: VersionConverter<Output = Self>;

    /// Execution handler for this action.
    ///
    /// For more details see the [`Handler`] trait.
    type Handler: Handler<Context = Self::Context, Action = Self>;

    /// Output returned by executing this action on the remote.
    ///
    /// Note this is only available if the action was executed on the remote via
    /// [`crate::queue::Queue::apply_action`].
    type RemoteOutput: Send;

    /// Output returned by executing this action on the local state.
    type LocalOutput: Send;

    /// Error type returned if this action fails.
    ///
    /// To ensure we can correctly implement network error detection errors need
    /// to implement the [`Error`] trait.
    type Error: Error + Send + From<WriterGuardError>;

    /// Type of the execution context associated with this action.
    ///
    /// If no context is necessary simply use `()`.
    type Context: Send + Sync + Any + 'static;

    /// Action version.
    fn version(&self) -> u32 {
        Self::VERSION
    }

    /// Action Type.
    fn action_type(&self) -> Type {
        Self::TYPE
    }

    /// Action priority.
    fn priority(&self) -> Priority {
        Self::PRIORITY
    }
}

/// This type exists to make sure that when we attempt to modify local state in the queue executor
/// we only do so if we have the permission to do so.
///
/// Permission is granted if the [`ExecutorGuard`] is still valid. This implementation
/// detail is abstracted away with this type to make future changes easier.
///
/// Database read queries can be made over this type as it implements `Deref<Target=Tether>`.
/// For writes use the [`transaction()`] method.
pub struct WriterGuard<'t> {
    tether: &'t mut Tether,
    execution_guard: &'t ExecutionGuard,
}

impl<'t> WriterGuard<'t> {
    pub(crate) fn new(tether: &'t mut Tether, execution_guard: &'t ExecutionGuard) -> Self {
        Self {
            tether,
            execution_guard,
        }
    }
    /// Create a new transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StashError`] if the transaction failed to be created  and [`WriterGuardError::Expired`]
    /// if this execution lock has expired.
    pub async fn tx<F, T, E>(&mut self, closure: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, E>,
        E: From<WriterGuardError> + From<StashError>,
    {
        self.execution_guard.tx(self.tether, closure).await
    }

    /// Access the tether for read only db queries.
    #[must_use]
    pub fn tether(&self) -> &Tether {
        self.tether
    }
}

impl RunTransaction for WriterGuard<'_> {
    #[allow(clippy::manual_async_fn)]
    fn run_tx<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>>
    where
        F: AsyncFnOnce(&Bond<'_>) -> Result<T, anyhow::Error>,
    {
        async {
            self.tether
                .tx(closure)
                .await
                .context("Could not create transaction for writerguard")
        }
    }
}

/// Possible [`WriterGuardErrors`]
#[derive(Debug, thiserror::Error)]
pub enum WriterGuardError {
    #[error("This executor lock has expired")]
    Expired,
    #[error("{0}")]
    Stash(#[from] StashError),
}

/// Defines how an action behaves.
///
/// To define the data on which an action operates see the [`Action`] trait.
#[allow(async_fn_in_trait)]
pub trait Handler: Default + 'static + Send + Sync {
    /// Action on which this handler operates.
    type Action: Action;

    type Context: Any + Send + Sync + 'static;

    /// Apply the `action` to the local database using the given `tx` transaction.
    ///
    /// # Remarks
    ///
    /// Changes made to the `action` data at this point will be persisted into the database once
    /// executing has finished successfully.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn apply_local(
        &self,
        this_id: ActionId,
        context: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error>,
    > + Send;

    /// Revert the `action` from the local database using the given `tx` transaction.
    ///
    /// This function is only called if:
    /// * Remote operation failed to execute and the resulting errors is not a network error.
    /// * The action is being cancelled.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn revert_local(
        &self,
        this_id: ActionId,
        context: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond,
    ) -> impl Future<Output = Result<(), <Self::Action as Action>::Error>> + Send;

    /// Apply the `action` on the server.
    ///
    /// Adjust local data if necessary.
    ///
    /// This function is always called after [`Handler::apply_local()`].
    ///
    /// # Remarks
    ///
    /// Changes made to the `action` data at this point are accessible to [`Handler::apply_local_post_remote()`]
    /// after this call. They are not serialized to the database.
    ///
    /// # Errors
    ///
    /// Returns error if the network request failed.
    fn apply_remote(
        &self,
        this_id: ActionId,
        context: &Self::Context,
        action: &mut Self::Action,
        writer_guard: WriterGuard<'_>,
    ) -> impl Future<
        Output = Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error>,
    > + Send;
}

/// Identifier for an action that has been queued.
///
/// This can be used to interact with certain API's available on the [`crate::queue::Queue`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ActionId(pub u64);

impl Display for ActionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<u64> for ActionId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl FromSql for ActionId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u64::column_result(value).map(ActionId)
    }
}

impl ToSql for ActionId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

/// A list of resources to associate with this action. Can be any of any type as
/// long as it is serializable.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct Resources(Vec<Vec<u8>>);

impl Resources {
    /// Get and decode a resource at the given `index`
    pub fn get<'de, T: Deserialize<'de>>(
        &'de self,
        index: usize,
    ) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::decode::from_slice(&self.0[index])
    }
}

impl Deref for Resources {
    type Target = Vec<Vec<u8>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Resources {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

sql_using_serde!(Resources);

/// Metadata associated with an [`Action`].
///
/// By default, [`Action`]s do not have any associated metadata. If you queue an action
/// it will be assigned a default contructed [`Metadata`].
///
/// You can construct a custom [`Metadata`] type to override certain behaviors and/or assign
/// more details to this action.
///
/// All the associated metadata is returned on error as [`QueuedMetadata`].
///
/// See also the [`MetadataBuilder`] type.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// List of queued actions the action depends upon. The action will only execute if all
    /// the dependencies have been executed.
    pub(crate) dependencies: Vec<ActionId>,
    /// Optional debug string which can be assigned to diagnose issues or provide more context.
    pub(crate) debug_string: Option<String>,
    /// A list of resources to associate with this action. Can be any of any type as long as it is
    /// serializable.
    pub(crate) resources: Resources,
    /// Time at which this action was created.
    pub(crate) created: chrono::DateTime<chrono::Utc>,
    /// If set, overrides the action's default priority.
    pub(crate) priority_override: Option<Priority>,
    /// If set, delays the execution of this action by the specified duration.
    pub(crate) delay: Option<std::time::Duration>,
    /// If set, overrides the group assigned to the action.
    pub(crate) group_override: Option<ActionGroup>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            dependencies: vec![],
            debug_string: None,
            resources: Resources::default(),
            created: chrono::Utc::now(),
            priority_override: None,
            delay: None,
            group_override: None,
        }
    }
}

impl Metadata {
    /// Create a new builder for this type.
    #[must_use]
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::new()
    }
}

/// Builder for [`Metadata`].
pub struct MetadataBuilder {
    metadata: Metadata,
}
impl Default for MetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataBuilder {
    /// Create new instance of the builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            metadata: Metadata::default(),
        }
    }

    /// Assign a `debug_string` to an action.
    #[must_use]
    pub fn with_debug_string(mut self, debug_string: impl Into<String>) -> Self {
        self.metadata.debug_string = Some(debug_string.into());
        self
    }

    /// Assign a `resource` to this action.
    ///
    /// The `resource` will be serialized into a byte array.
    ///
    /// # Errors
    ///
    /// Returns error if the serialization failed.
    pub fn with_resource(
        mut self,
        resource: &impl Serialize,
    ) -> Result<Self, rmp_serde::encode::Error> {
        self.metadata.resources.push(rmp_serde::to_vec(resource)?);
        Ok(self)
    }

    /// Assign `action_id` as a dependency of this action.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// `action_id` has completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_dependency(mut self, action_id: ActionId) -> Self {
        self.metadata.dependencies.push(action_id);
        self
    }

    /// Assign `action_ids` as a dependency of this action.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// all the actions in `action_ids` have completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_dependencies(mut self, action_ids: impl IntoIterator<Item = ActionId>) -> Self {
        self.metadata.dependencies.extend(action_ids);
        self
    }

    /// Override the creation time of an action.
    ///
    /// By default, the action will use the current time at the time of queueing.
    #[must_use]
    pub fn with_creation_time(mut self, date_time: chrono::DateTime<chrono::Utc>) -> Self {
        self.metadata.created = date_time;
        self
    }

    /// Override the priority of an action.
    #[must_use]
    pub fn with_priority_override(mut self, priority: Priority) -> Self {
        self.metadata.priority_override = Some(priority);
        self
    }

    /// Delay the execution of this action by `duration`.
    ///
    /// Delaying the action still causes [`Handler::apply_local`] to be called, but subsequent
    /// calls are delayed until the action has spent at least a period of `duration` in
    /// the queue.
    ///
    /// Note that this requires the action to be queued
    /// ([`queue_action()`](crate::queue::Queue::queue_action()))
    /// rather than applied ([`apply_action()`](crate::queue::Queue::apply_action())).
    #[must_use]
    pub fn with_delay(mut self, duration: std::time::Duration) -> Self {
        self.metadata.delay = Some(duration);
        self
    }

    /// Override the default group assigned to this action with `group`.
    #[must_use]
    pub fn with_group_override(mut self, group: ActionGroup) -> Self {
        self.metadata.group_override = Some(group);
        self
    }

    /// Generate the [`Metadata`] type.
    #[must_use]
    pub fn build(self) -> Metadata {
        self.metadata
    }
}

/// Errors that can occur during factory operations.
#[derive(Debug, thiserror::Error)]
pub enum FactoryError {
    #[error("Stored action {0} has unknown action type: {1}")]
    UnknownType(ActionId, String),
    #[error("Action has invalid version {0}")]
    InvalidVersion(u32),
    #[error("Failed to deserialize: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
    #[error("Version Conversion: {0}")]
    VersionConverter(#[from] VersionConverterError),
    #[error("Action type {0} is already registered")]
    AlreadyRegistered(Type),
    #[error("Unknown error:{0}")]
    Unknown(anyhow::Error),
}

pub type FactoryResult<T> = Result<T, FactoryError>;

/// Internal trait used by the [`Factory`] to convert a [`StoredAction`] into a [`QueuedAction`].
trait Decoder: Send + Sync {
    /// Convert the stored `action` into a [`QueuedAction`] which can be executed by the queue.
    fn decode(
        &self,
        action: StoredAction,
    ) -> FactoryResult<(Box<dyn QueuedAction>, Arc<QueuedMetadata>)>;
}

/// [`Decoder`] implementation that works for any [`Action`] type.
struct TypeErasedDecoder<T: Action> {
    p: PhantomData<fn() -> T>,
}

impl<T: Action> Decoder for TypeErasedDecoder<T> {
    fn decode(
        &self,
        stored_action: StoredAction,
    ) -> FactoryResult<(Box<dyn QueuedAction>, Arc<QueuedMetadata>)> {
        // Check version
        let deserialized: T =
            T::VersionConverter::convert(stored_action.version, T::VERSION, &stored_action.state)?;

        let id = stored_action.id.unwrap();
        let queued_metadata = QueuedMetadata::from(stored_action);

        // Return type.
        Ok((
            Box::new(TypeErasedAction::<T> {
                action_id: id,
                handler: T::Handler::default(),
                action: deserialized,
            }),
            Arc::new(queued_metadata),
        ))
    }
}

/// A Factory pattern implementation for [`Action`]s which are stored on the [`Queue`](crate::queue::Queue).
///
/// When action are stored on the queue, their state is serialized into the database. In order to
/// be able to decode an execute those actions they need to be registered with a factory instance.
///
#[derive(Default)]
pub struct Factory {
    factories: HashMap<String, Box<dyn Decoder>>,
}

impl Factory {
    /// Create a new instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Check whether the given action is registered with the queue.
    #[must_use]
    pub fn has_action<T: Action>(&self) -> bool {
        self.factories.contains_key(T::TYPE.as_ref())
    }

    /// Register an [`Action`] with the factory.
    ///
    /// # Errors
    ///
    /// Returns error if the action type was already registered before.
    pub fn register<T: Action>(&mut self) -> FactoryResult<()> {
        match self.factories.entry(T::TYPE.to_string()) {
            Entry::Occupied(_) => Err(FactoryError::AlreadyRegistered(T::TYPE)),
            Entry::Vacant(v) => {
                v.insert(Box::new(TypeErasedDecoder::<T> { p: PhantomData }));
                Ok(())
            }
        }
    }

    /// Decode the stored action.
    ///
    /// # Errors
    ///
    /// Returns error if the decoding failed.
    pub(crate) fn decode(
        &self,
        action: StoredAction,
    ) -> FactoryResult<(Box<dyn QueuedAction>, Arc<QueuedMetadata>)> {
        let Some(factory) = self.factories.get(&action.action_type) else {
            return Err(FactoryError::UnknownType(
                action.id.unwrap(),
                action.action_type.clone(),
            ));
        };

        factory.decode(action)
    }
}

#[derive(Debug, thiserror::Error)]
pub struct NoopError {}

impl Display for NoopError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Noop")
    }
}

impl Error for NoopError {
    fn is_network_failure(&self) -> bool {
        false
    }

    fn is_writer_guard_expired(&self) -> bool {
        false
    }
}

impl From<WriterGuardError> for NoopError {
    fn from(_: WriterGuardError) -> Self {
        Self {}
    }
}

/// Serialize the `action` to a binary format.
///
/// # Errors
///
/// Returns error if the serialization failed.
pub(crate) fn serialize<T: Action>(action: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(action)
}

/// Deserialize and action from `data`.
///
/// # Errors
///
/// Returns error if the deserialization failed.
pub fn deserialize<T: Action>(data: &[u8]) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(data)
}
