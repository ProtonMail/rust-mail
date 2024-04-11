use crate::{SessionProvider, SessionProviderError, StoredAction, StoredActionId};
use proton_api_core::exports::anyhow;
use proton_api_core::exports::serde;
use proton_api_core::exports::thiserror;
use proton_sqlite3::rusqlite::types::{
    FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef,
};
use proton_sqlite3::rusqlite::ToSql;
use proton_sqlite3::{rusqlite, SqliteTransaction};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("Local Source: {0}")]
    Local(#[source] anyhow::Error),
    #[error("Local Source: {0}")]
    Remote(#[from] proton_api_core::http::RequestError),
    #[error("Serialization error: {0}")]
    Serialization(#[source] rmp_serde::encode::Error),
    #[error("Unknown Error: {0}")]
    Unknown(#[source] anyhow::Error),
}

/// ActionId is a unique identifier for each action type. This is required to construct an
/// action after it has been serialized to the queue. [`std::any::TypeId`] is not guaranteed to remain
/// the same between rust releases.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ActionId(pub Uuid);

impl ActionId {
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Display for ActionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl FromSql for ActionId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Uuid::column_result(value).map(ActionId)
    }
}

impl ToSql for ActionId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

/// Generate a new ActionId from an UUID string literal.
/// ```
/// use proton_action_queue::{define_action_id};
///
/// define_action_id!(MY_PRIVATE_ACTION_ID, "831f9eb6-5238-4f0b-a0ff-68afce98e119");
/// define_action_id!(pub MY_PUBLIC_ACTION_ID, "831f9eb6-5238-4f0b-a0ff-78afce98e119");
/// ```
#[macro_export]
macro_rules! define_action_id {
    ($name:ident, $uuid_str:literal) => {
        const $name: $crate::ActionId = $crate::ActionId::new(uuid::uuid!($uuid_str));
    };
    ($viz:vis $name:ident, $uuid_str:literal) => {
        $viz const $name: $crate::ActionId = $crate::ActionId::new(uuid::uuid!($uuid_str));
    };
}

/// Defines the priority of a queued action.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum ActionPriority {
    Highest = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

impl ToSql for ActionPriority {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for ActionPriority {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(ActionPriority::Highest),
            1 => Ok(ActionPriority::High),
            2 => Ok(ActionPriority::Normal),
            3 => Ok(ActionPriority::Low),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

/// Result of checking the local state before applying an action.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ActionLocalValidationResult {
    /// The state is valid and the action can be applied.
    Valid,
    /// The state is no longer valid, action should not be applied.
    Invalid,
}

pub type ActionResult<T> = Result<T, ActionError>;

/// Defines an action in the queue. Action behavior is controlled with the [`LocalActionHandler`]
/// and the [`RemoteActionHandler`] traits.
pub trait Action: Any + Serialize + DeserializeOwned + Debug {
    /// Return the ActionId, generate one with the
    const ID: ActionId;
    const VERSION: u32;
    const PRIORITY: ActionPriority = ActionPriority::Normal;
    fn action_version(&self) -> u32 {
        Self::VERSION
    }

    fn action_id(&self) -> &'static ActionId {
        &Self::ID
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn priority(&self) -> ActionPriority {
        Self::PRIORITY
    }
}

/// Define the behavior of a queued action for local changes. These will be instantiated by an [`ActionFactoryInstance`].
pub trait LocalActionHandler {
    /// Apply the action to the local state.
    fn apply_local(&mut self) -> ActionResult<()>;
}

/// Define the behavior of a queued action for remote changes. These will be instantiated by an [`ActionFactoryInstance`].
pub trait RemoteActionHandler {
    /// Revert the action on the local state.
    fn revert_local(&mut self) -> ActionResult<()>;

    /// Check whether the local state still matches this action's expectations.
    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult>;

    /// Apply the changes on the remote.
    fn apply_remote(&mut self) -> ActionResult<()>;
}

/// Errors that can occur during action factory operations.
#[derive(Debug, thiserror::Error)]
pub enum ActionFactoryError {
    #[error("Action has unknown type: {0}")]
    UnknownAction(ActionId),
    #[error("Stored action {0} has unknown action type: {1}")]
    UnknownStoredAction(StoredActionId, ActionId),
    #[error("Failed to create local handler for action {0}: {1}")]
    LocalHandler(ActionId, ActionFactoryInstanceError),
    #[error("Stored action {0} ({1}) failed to create remote handler: {2}")]
    RemoteHandler(StoredActionId, ActionId, ActionFactoryInstanceError),
    #[error("Unknown error:{0}")]
    Unknown(anyhow::Error),
}

/// Errors that can occur during action factory instance operations.
#[derive(Debug, thiserror::Error)]
pub enum ActionFactoryInstanceError {
    #[error("Action has invalid version {0}")]
    InvalidVersion(u32),
    #[error("Action is not of expected type got '{0:?}', expected '{1:?}'")]
    InvalidType(TypeId, TypeId),
    #[error("Failed to deserialize: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
    #[error("Failed to retrieve session: {0}")]
    SessionProvider(#[from] SessionProviderError),
    #[error("Unknown error: {0}")]
    Unknown(anyhow::Error),
}

/// A factory for the creation of [`LocationActionHandler`] and [`RemoteActionHandler`] for an action.
/// It's recommended to store any mocking/interface/wrappers in the factory and then share them
/// with each of the handlers in order to keep the actions themselves as simple as possible.
pub trait ActionFactoryInstance: Debug + Send + Sync {
    /// Action id for this handler.
    fn action_id(&self) -> &'static ActionId;

    /// Construct a new [`LocalActionHandler`] for an action
    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        tx: &'r mut SqliteTransaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError>;

    /// Construct a new [`RemoteActionHandler`] for a stored action.
    fn remote_handler<'r, 't: 'r>(
        &'r self,
        action: &StoredAction,
        tx: &'r mut SqliteTransaction<'t>,
        session_provider: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryInstanceError>;
}

/// Gateway to all [`ActionFactoryInstance`] types. Each action should register their handler
/// with this type.
#[derive(Default)]
pub struct ActionFactory {
    factories: HashMap<ActionId, Box<dyn ActionFactoryInstance>>,
}

impl ActionFactory {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register an [`ActionFactoryInstance`] with this factory. If an instance already exists for
    /// this type and error is returned with the supplied value.
    pub fn register(
        &mut self,
        factory: Box<dyn ActionFactoryInstance>,
    ) -> Result<(), Box<dyn ActionFactoryInstance>> {
        match self.factories.entry(factory.action_id().clone()) {
            Entry::Occupied(_) => Err(factory),
            Entry::Vacant(v) => {
                v.insert(factory);
                Ok(())
            }
        }
    }

    /// Get a local handler for a given action.
    pub fn local_handler<'r, 't: 'r, T: Action>(
        &self,
        action: &'r T,
        tx: &'r mut SqliteTransaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryError> {
        let Some(factory) = self.factories.get(action.action_id()) else {
            return Err(ActionFactoryError::UnknownAction(
                action.action_id().clone(),
            ));
        };

        factory
            .local_handler(action.as_any(), tx)
            .map_err(|e| ActionFactoryError::LocalHandler(action.action_id().clone(), e))
    }

    /// Get a remote handler for a stored action.
    pub fn remote_handler<'r, 't: 'r>(
        &'r self,
        action: &StoredAction,
        tx: &'r mut SqliteTransaction<'t>,
        session_provider: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryError> {
        let Some(factory) = self.factories.get(&action.action_id) else {
            return Err(ActionFactoryError::UnknownStoredAction(
                action.id,
                action.action_id.clone(),
            ));
        };

        factory
            .remote_handler(action, tx, session_provider)
            .map_err(|e| ActionFactoryError::RemoteHandler(action.id, action.action_id.clone(), e))
    }
}
