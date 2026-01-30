use crate::db::{ActionDependency, ExecutionGuard, StoredAction};
use crate::queue::{
    ActionError, ActionRequeueReason, ErasedQueuedAction, QueuedAction, QueuedActionOutput,
    QueuedMetadata,
};
use crate::rebase::RebaseChangeSet;
use anyhow::Context;
use derive_more::derive::TryFrom;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Transaction, Value,
    ValueRef,
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

/// While actions can return any error type, for better user experience it makes
/// sense to distinguish "fatal errors" from "retryable errors" - this is what
/// this trait allows.
///
/// See [`Self::requeueable()`] for details.
pub trait Error: std::error::Error + Send + Sync {
    /// If this error is not fatal (e.g. a temporary networking issue), this
    /// method returns `Some` - doing so causes the action to be requeued and
    /// retried on the next opportunity.
    ///
    /// If this error is fatal (e.g. missing database table), this method
    /// returns `None` - doing so causes the action to be cancelled.
    fn can_requeue(&self) -> Option<ActionRequeueReason>;
}

#[derive(Debug, thiserror::Error)]
pub enum VersionConverterError {
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
    Lowest = 4,
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
pub trait VersionConverter<Db: stash::marker::DatabaseMarker> {
    /// Output of the conversion.
    type Output: Action<Db>;
    /// Convert the serialized action from the `old_version` to the `new_version`.
    ///
    /// The `data` for this action was recorded when the action was at `old_version`.
    ///
    /// # Remarks
    /// This function is also called if `old_version` and `new_version` have the samve value.
    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output>;
}

/// Default version converter implementation.
///
/// If the versions don't match it will throw an error.
pub struct DefaultVersionConverter<T>(PhantomData<T>);

impl<T, Db> VersionConverter<Db> for DefaultVersionConverter<T>
where
    T: Action<Db>,
    Db: stash::marker::DatabaseMarker,
{
    type Output = T;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        if old_version == current_version {
            Ok(deserialize::<T>(data)?)
        } else {
            Err(FactoryError::InvalidVersion(current_version))
        }
    }
}

/// A dependency key is automatic dependency tracking key which will be used to assign
/// dependencies to this action when it is saved.
///
/// This aims to reduce the burden on the users to remember when what the last action id of
/// an action they queued to specify a dependency.
///
/// For instances, imagine whe have the following sequence of actions.
///
/// * Create Resource A -> Id1
/// * Do something which depends on Resource A to Resource B -> Id2
/// * Do something else with Resource B that depends on the previous action (Id2) -> Id3
///
/// Without dependency keys, we would need to store Id1, Id2 and Id3 somewhere and specify the
/// dependencies at creation time.
///
/// With dependency keys, each action can specify a key which other actions can automatically
/// depend on them if they specify the same key. Using the same example above in order:
///
/// * Id1 creates dependency key: `Key-Resource1`
/// * Id2 specifies a dependency on `Key-Resource1` and `Key-Resource2`
/// * Id3 specifies a dependency on `Key-Resource2`
///
/// When these actions are saved, we will save the action id of the current action with this
/// resource key and add the existing value to the action's dependency.
///
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ActionDependencyKey(String);

impl ActionDependencyKey {
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<T: Into<String>> From<T> for ActionDependencyKey {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl Display for ActionDependencyKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for ActionDependencyKey {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

impl FromSql for ActionDependencyKey {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ActionDependencyKeys {
    /// Required dependency keys that this action would like to depended on. Required dependencies
    /// will cause revert on failure.
    pub required: Vec<ActionDependencyKey>,
    /// Optional keys that this action would like to depend on. Does not cause revert if the
    /// dependency fails to execute.
    pub optional: Vec<ActionDependencyKey>,
    /// Record extra dependency keys for this action. Keys specified here do not introduce
    /// any dependencies, but can be used by other actions.
    pub record: Vec<ActionDependencyKey>,
}

/// Required metadata to define an action.
///
/// An Action is an operation in the system that is applied opportunistically to the local data set
/// first and executed on the remote server as soon as possible.
///
/// Its is recommended to assign this trait to the part of the action which contains the
/// data required for it to operate on. Execution of the action is defined by the [`Handler`] trait.
///
pub trait Action<Db: stash::marker::DatabaseMarker>:
    Serialize + DeserializeOwned + 'static + Send
{
    const TYPE: Type;
    const GROUP: ActionGroup = ActionGroup::default();
    const VERSION: u32;

    /// Default priority for this action.
    ///
    /// Can be overridden with [`MetadataBuilder::with_priority_override`].
    const PRIORITY: Priority = Priority::Normal;
    const MAX_RETRIES: Option<u32> = None;

    /// Associated version converter.
    ///
    /// For more details see the [`VersionConverter`] trait.
    type VersionConverter: VersionConverter<Db, Output = Self>;

    /// Execution handler for this action.
    ///
    /// For more details see the [`Handler`] trait.
    type Handler: Handler<Db, Action = Self>;

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

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn action_type(&self) -> Type {
        Self::TYPE
    }

    fn priority(&self) -> Priority {
        Self::PRIORITY
    }

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeys::default()
    }
}

#[allow(type_alias_bounds, reason = "This is only used for convenience")]
pub type LocalOutput<Db, T: Action<Db>> = Result<QueuedActionOutput<T, Db>, ActionError<T, Db>>;

/// This type exists to make sure that when we attempt to modify local state in the queue executor
/// we only do so if we have the permission to do so.
///
/// Permission is granted if the [`ExecutorGuard`] is still valid. This implementation
/// detail is abstracted away with this type to make future changes easier.
///
/// Database read queries can be made over this type as it implements `Deref<Target=Tether>`.
/// For writes use the [`transaction()`] method.
pub struct WriterGuard<'t, Db: stash::marker::DatabaseMarker> {
    tether: &'t mut Tether<Db>,
    execution_guard: &'t ExecutionGuard,
}

impl<'t, Db: stash::marker::DatabaseMarker> WriterGuard<'t, Db> {
    pub(crate) fn new(tether: &'t mut Tether<Db>, execution_guard: &'t ExecutionGuard) -> Self {
        Self {
            tether,
            execution_guard,
        }
    }

    pub async fn tx<F, T, E>(&mut self, closure: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&Bond<'_, Db>) -> Result<T, E>,
        E: From<WriterGuardError> + From<StashError>,
    {
        self.execution_guard.tx(self.tether, closure).await
    }

    /// Access the tether for read only db queries.
    #[must_use]
    pub fn tether(&self) -> &Tether<Db> {
        self.tether
    }
}

impl<Db: stash::marker::DatabaseMarker> RunTransaction<Db> for WriterGuard<'_, Db> {
    fn tether(&self) -> &Tether<Db> {
        self.tether
    }

    #[allow(clippy::manual_async_fn)]
    fn run_tx<T, F>(&mut self, closure: F) -> impl Future<Output = anyhow::Result<T>>
    where
        F: AsyncFnOnce(&Bond<'_, Db>) -> Result<T, anyhow::Error>,
    {
        async {
            self.tether
                .tx(closure)
                .await
                .context("Could not create transaction for writerguard")
        }
    }

    async fn run_tx_sync<T, F>(&mut self, closure: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Transaction<'_>) -> stash::stash::StashResult<T> + Send + 'static,
        T: Send + 'static,
    {
        self.tether
            .sync_tx_returning(closure)
            .await
            .context("Could not create transaction for writerguard")
    }
}

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
pub trait Handler<Db: stash::marker::DatabaseMarker>: Send + Sync {
    type Action: Action<Db>;

    /// Apply the `action` to the local database using the given `tx` transaction.
    ///
    /// # Remarks
    ///
    /// Changes made to the `action` data at this point will be persisted into the database once
    /// executing has finished successfully.
    fn apply_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_, Db>,
    ) -> impl Future<
        Output = Result<
            <Self::Action as Action<Db>>::LocalOutput,
            <Self::Action as Action<Db>>::Error,
        >,
    > + Send;

    /// Revert the `action` from the local database using the given `tx` transaction.
    ///
    /// This function is only called if:
    /// * Remote operation failed to execute and the resulting errors is not a network error.
    /// * The action is being cancelled.
    fn revert_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_, Db>,
    ) -> impl Future<Output = Result<(), <Self::Action as Action<Db>>::Error>> + Send;

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
    fn apply_remote(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        writer_guard: WriterGuard<'_, Db>,
    ) -> impl Future<
        Output = Result<
            <Self::Action as Action<Db>>::RemoteOutput,
            <Self::Action as Action<Db>>::Error,
        >,
    > + Send;

    /// Rebase local changes over the current state of the cached data.
    ///
    /// This method will be invoked when new data is brought in from the server at the discretion
    /// of the integrator.
    ///
    /// # Remarks
    ///
    /// * Action state can be updated and will be saved back to the queue.
    /// * Rebasing can happen on running actions while they are executing `apply_remote`.
    fn rebase_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        change_set: &RebaseChangeSet,
        tx: &Bond<'_, Db>,
    ) -> impl Future<Output = Result<(), <Self::Action as Action<Db>>::Error>> + Send;
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
    pub(crate) dependencies: Vec<ActionDependency>,
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
    #[must_use]
    pub fn builder() -> MetadataBuilder {
        MetadataBuilder::new()
    }

    #[must_use]
    pub fn with_dependency(dependency_id: ActionId) -> Self {
        MetadataBuilder::new()
            .with_dependency(dependency_id)
            .build()
    }
}

pub struct MetadataBuilder {
    metadata: Metadata,
}

impl Default for MetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataBuilder {
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
    pub fn with_resource(
        mut self,
        resource: &impl Serialize,
    ) -> Result<Self, rmp_serde::encode::Error> {
        self.metadata.resources.push(rmp_serde::to_vec(resource)?);
        Ok(self)
    }

    /// Assign `action_id` as a sequential dependency of this action.
    ///
    /// If the dependent action fails, this action will still be executed.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// `action_id` has completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_optional_dependency(mut self, action_id: ActionId) -> Self {
        self.metadata
            .dependencies
            .push(ActionDependency::optional(action_id));
        self
    }

    /// Assign `action_ids` as a sequential dependency of this action.
    ///
    /// If the dependent action fails, this action will still be executed.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// all the actions in `action_ids` have completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_optional_dependencies(
        mut self,
        action_ids: impl IntoIterator<Item = ActionId>,
    ) -> Self {
        self.metadata
            .dependencies
            .extend(action_ids.into_iter().map(ActionDependency::optional));
        self
    }

    /// Assign `action_id` as a dependency of this action.
    ///
    /// If the dependent action fails, this action will not be executed and local state will
    /// be reverted.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// `action_id` has completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_dependency(mut self, action_id: ActionId) -> Self {
        self.metadata
            .dependencies
            .push(ActionDependency::required(action_id));
        self
    }

    /// Assign `action_ids` as a dependency of this action.
    ///
    /// If the dependent action fails, this action will not be executed and local state will
    /// be reverted.
    ///
    /// The action to which this metadata will be assigned will not execute until
    /// all the actions in `action_ids` have completed.
    ///
    /// This function is cumulative and  will not override previous values if called
    /// multiple times.
    #[must_use]
    pub fn with_dependencies(mut self, action_ids: impl IntoIterator<Item = ActionId>) -> Self {
        self.metadata
            .dependencies
            .extend(action_ids.into_iter().map(ActionDependency::required));
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
}

pub type FactoryResult<T> = Result<T, FactoryError>;
pub(crate) type DecodedAction<Db> = (Box<dyn ErasedQueuedAction<Db>>, Arc<QueuedMetadata>);

/// A Factory pattern implementation for [`Action`]s which are stored on the [`Queue`](crate::queue::Queue).
///
/// When action are stored on the queue, their state is serialized into the database. In order to
/// be able to decode an execute those actions they need to be registered with a factory instance.
///
pub struct Factory<Db: stash::marker::DatabaseMarker> {
    actions: HashMap<String, ActionFactory<Db>>,
}

impl<Db: stash::marker::DatabaseMarker> Default for Factory<Db> {
    fn default() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }
}

impl<Db: stash::marker::DatabaseMarker> Factory<Db> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn handler<T: Action<Db>>(&self) -> Option<Arc<T::Handler>> {
        self.actions
            .get(T::TYPE.0)
            .and_then(|action| Arc::downcast(action.handler.clone()).ok())
    }

    pub fn register<T: Action<Db>>(&mut self, handler: T::Handler) -> FactoryResult<()> {
        match self.actions.entry(T::TYPE.to_string()) {
            Entry::Occupied(_) => Err(FactoryError::AlreadyRegistered(T::TYPE)),

            Entry::Vacant(action) => {
                let handler = Arc::new(handler);

                #[allow(trivial_casts, reason = "false-positive")]
                let decoder = Box::new({
                    let handler = handler.clone();

                    move |stored_action: StoredAction<Db>| {
                        let action: T = T::VersionConverter::convert(
                            stored_action.version,
                            T::VERSION,
                            &stored_action.state,
                        )?;

                        let id = stored_action.id.unwrap();
                        let meta = QueuedMetadata::from(stored_action);

                        Ok((
                            Box::new(QueuedAction {
                                id,
                                action,
                                handler: handler.clone(),
                            }) as Box<dyn ErasedQueuedAction<Db>>,
                            Arc::new(meta),
                        ))
                    }
                });

                action.insert(ActionFactory { decoder, handler });

                Ok(())
            }
        }
    }
    pub fn register_or_replace<T: Action<Db>>(&mut self, handler: T::Handler) {
        let handler = Arc::new(handler);

        #[allow(trivial_casts, reason = "false-positive")]
        let decoder = Box::new({
            let handler = handler.clone();

            move |stored_action: StoredAction<Db>| {
                let action: T = T::VersionConverter::convert(
                    stored_action.version,
                    T::VERSION,
                    &stored_action.state,
                )?;

                let id = stored_action.id.unwrap();
                let meta = QueuedMetadata::from(stored_action);

                Ok((
                    Box::new(QueuedAction {
                        id,
                        action,
                        handler: handler.clone(),
                    }) as Box<dyn ErasedQueuedAction<Db>>,
                    Arc::new(meta),
                ))
            }
        });

        self.actions
            .insert(T::TYPE.to_string(), ActionFactory { decoder, handler });
    }

    pub(crate) fn decode(&self, action: StoredAction<Db>) -> FactoryResult<DecodedAction<Db>> {
        let Some(factory) = self.actions.get(&action.action_type) else {
            return Err(FactoryError::UnknownType(
                action.id.unwrap(),
                action.action_type.clone(),
            ));
        };

        (factory.decoder)(action)
    }
}

struct ActionFactory<Db: stash::marker::DatabaseMarker> {
    decoder: Box<dyn Fn(StoredAction<Db>) -> FactoryResult<DecodedAction<Db>> + Send + Sync>,
    handler: Arc<dyn Any + Send + Sync + 'static>,
}

#[derive(Debug, thiserror::Error)]
pub struct NoopError {}

impl Display for NoopError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Noop")
    }
}

impl Error for NoopError {
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        None
    }
}

impl From<WriterGuardError> for NoopError {
    fn from(_: WriterGuardError) -> Self {
        Self {}
    }
}

pub(crate) fn serialize<T: Action<Db>, Db: stash::marker::DatabaseMarker>(
    action: &T,
) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(action)
}

pub fn deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(data)
}
