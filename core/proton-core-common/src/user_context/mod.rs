use proton_api_core::domain::UserId;
use proton_api_core::Session;
use proton_sqlite3::MigratorError;
use stash::stash::Stash;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

pub use self::keys::*;
mod addresses;
mod keys;
mod user;

/// Extra initializer for the user database.
pub trait UserDatabaseInitializer: Send + Sync {
    /// Initialize the database as needed by running database migrations.
    ///
    /// # Errors
    /// Return error if the migration failed.
    fn initialize(&self, stash: &Stash) -> Result<(), MigratorError>;
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    session: Session,
    stash: Stash,
    user_id: UserId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    pub(crate) fn new(session: Session, stash: Stash, id: UserId) -> Self {
        Self {
            session,
            stash,
            user_id: id,
            key_manager: Arc::new(CryptoKeyManager::new()),
        }
    }

    /// Get the network session.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get the network session converted to a type that accepts this type.
    #[must_use]
    pub fn session_as<T: From<Session>>(&self) -> T {
        T::from(self.session.clone())
    }

    /// Get the tracker service for database operations.
    #[must_use]
    pub fn tracker_service(&self) -> &Stash {
        &self.stash
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
}
