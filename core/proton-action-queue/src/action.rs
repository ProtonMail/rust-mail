use crate::db::StoredAction;
use crate::queue::{QueuedAction, QueuedMetadata, TypeErasedAction};
use proton_api_core::service::ApiServiceError;
use proton_api_core::session::Session;
use serde::de::DeserializeOwned;
use serde::Serialize;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::stash::{Stash, Tether};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;

/// Actions can return any error type, but we need to be able to inspect the http request error
/// to detect network failure.
pub trait Error: std::error::Error + Send + Sync {
    /// If the error contains a request error, return a reference to this error for inspection.
    fn request_error(&self) -> Option<&ApiServiceError>;

    /// Check if the error is the result of a network failure.
    ///
    /// An error is considered a network failure the server replies with 429/5xx HTTP status codes
    /// or there was an issue with the underlying network transport layer.
    #[must_use]
    fn is_network_failure(&self) -> bool {
        let Some(request_error) = self.request_error() else {
            return false;
        };

        request_error.is_network_failure()
    }
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

/// Defines the priority of a queued action.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
        match i64::column_result(value)? {
            0 => Ok(Priority::Highest),
            1 => Ok(Priority::High),
            2 => Ok(Priority::Normal),
            3 => Ok(Priority::Low),
            v => Err(FromSqlError::OutOfRange(v)),
        }
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
pub trait Action: Serialize + DeserializeOwned + 'static {
    /// Unique type identifier.
    const TYPE: Type;

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
    type Handler: Handler<Action = Self>;

    /// Output returned by executing this action.
    ///
    /// Note this is only available if the action was executed on the remote via
    /// [`crate::queue::Queue::apply_action`].
    type Output;

    /// Error type returned if this action fails.
    ///
    /// To ensure we can correctly implement network error detection errors need
    /// to implement the [`Error`] trait.
    type Error: Error;

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

/// Defines how an action behaves.
///
/// To define the data on which an action operates see the [`Action`] trait.
#[allow(async_fn_in_trait)]
pub trait Handler: Default + 'static {
    /// Action on which this handler operates.
    type Action: Action;

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
    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error>;

    /// Revert the `action` from the local database using the given `tx` transaction.
    ///
    /// This function is only called if:
    /// * Remote operation failed to execute and the resulting errors is not a network error.
    /// * The action is being cancelled.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error>;

    /// Apply the `action` on the server with the given `session`.
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
    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error>;
}

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
    pub(crate) dependencies: Vec<u64>,
    /// Optional debug string which can be assigned to diagnose issues or provide more context.
    pub(crate) debug_string: Option<String>,
    /// A list of resources to associate with this action. Can be any of any type as long as it is
    /// serializable.
    pub(crate) resources: Vec<Vec<u8>>,
    /// Time at which this action was created.
    pub(crate) created: chrono::DateTime<chrono::Utc>,
    /// If set, overrides the action's default priority.
    pub(crate) priority_override: Option<Priority>,
    /// If set, delays the execution of this action by the specified duration.
    pub(crate) delay: Option<std::time::Duration>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            dependencies: vec![],
            debug_string: None,
            resources: vec![],
            created: chrono::Utc::now(),
            priority_override: None,
            delay: None,
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
    pub fn with_dependency(mut self, action_id: u64) -> Self {
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
    pub fn with_dependencies(mut self, action_ids: impl IntoIterator<Item = u64>) -> Self {
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
    UnknownType(u64, String),
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
    ) -> FactoryResult<(Box<dyn QueuedAction>, QueuedMetadata)>;
}

/// [`Decoder`] implementation that works for any [`Action`] type.
struct TypeErasedDecoder<T: Action>(PhantomData<fn() -> T>);

impl<T: Action> Decoder for TypeErasedDecoder<T> {
    fn decode(
        &self,
        stored_action: StoredAction,
    ) -> FactoryResult<(Box<dyn QueuedAction>, QueuedMetadata)> {
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
            queued_metadata,
        ))
    }
}

/// A Factory pattern implementation for [`Action`]s which are stored on the
/// [`Queue`](crate::queue::Queue).
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

    /// Register an [`Action`] with the factory.
    ///
    /// # Errors
    ///
    /// Returns error if the action type was already registered before.
    pub fn register<T: Action + 'static>(&mut self) -> FactoryResult<()> {
        match self.factories.entry(T::TYPE.to_string()) {
            Entry::Occupied(_) => Err(FactoryError::AlreadyRegistered(T::TYPE)),
            Entry::Vacant(v) => {
                v.insert(Box::new(TypeErasedDecoder::<T>(PhantomData)));
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
    ) -> FactoryResult<(Box<dyn QueuedAction>, QueuedMetadata)> {
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
    fn request_error(&self) -> Option<&ApiServiceError> {
        None
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
