use proton_api_core::domain::UserId;
use proton_api_core::Session;
use proton_core_db::proton_sqlite3::{
    InProcessTrackerService, SqliteConnection, SqliteConnectionPool, TrackingConnection,
};
use proton_core_db::{CoreSqliteConnection, DBMigrationError, DBResult};
use std::fmt::{Debug, Formatter};

mod settings;

/// Extra initializer for the user database.
pub trait UserDatabaseInitializer: Send + Sync {
    fn initialize(&self, conn: &mut SqliteConnection) -> Result<(), DBMigrationError>;
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    session: Session,
    db_tracker: InProcessTrackerService,
    user_id: UserId,
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
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_as<T: From<Session>>(&self) -> T {
        T::from(self.session.clone())
    }

    pub fn new_db_connection(&self) -> DBResult<CoreSqliteConnection> {
        self.new_db_connection_as::<CoreSqliteConnection>()
    }

    pub fn new_db_connection_as<T: From<TrackingConnection>>(&self) -> DBResult<T> {
        self.db_tracker.new_connection().map(|c| c.into())
    }

    pub fn tracker_service(&self) -> &InProcessTrackerService {
        &self.db_tracker
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }
}
