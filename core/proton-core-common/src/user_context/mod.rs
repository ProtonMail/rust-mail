use crate::db::proton_sqlite3::{
    InProcessTrackerService, SqliteConnection, SqliteConnectionPool, TrackingConnection,
};
use crate::db::{CoreSqliteConnection, DBMigrationError, DBResult};
use proton_api_core::domain::UserId;
use proton_api_core::Session;
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
    fn initialize(&self, conn: &mut SqliteConnection) -> Result<(), DBMigrationError>;
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    session: Session,
    db_tracker: InProcessTrackerService,
    user_id: UserId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    pub(crate) fn new(
        session: Session,
        db_pool: SqliteConnectionPool,
        id: UserId,
    ) -> std::io::Result<Self> {
        Ok(Self {
            session,
            db_tracker: InProcessTrackerService::new(db_pool)?,
            user_id: id,
            key_manager: Arc::new(CryptoKeyManager::new()),
        })
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

    /// Get a new core db connection.
    ///
    /// # Errors
    /// Returns error if the connection can not be acquired.
    pub fn new_db_connection(&self) -> DBResult<CoreSqliteConnection> {
        self.new_db_connection_as::<CoreSqliteConnection>()
    }

    /// Get a new db connection of a given type.
    ///
    /// # Errors
    /// Returns error if the connection can not be acquired.
    pub fn new_db_connection_as<T: From<TrackingConnection>>(&self) -> DBResult<T> {
        self.db_tracker.new_connection().map(Into::into)
    }

    /// Get the tracker service for database operations.
    #[must_use]
    pub fn tracker_service(&self) -> &InProcessTrackerService {
        &self.db_tracker
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
}
