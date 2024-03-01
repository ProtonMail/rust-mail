use proton_api_core::domain::UserId;
use proton_api_core::Session;
use proton_core_db::proton_sqlite3::{
    InProcessTrackerService, SqliteConnection, SqliteConnectionPool, TrackingConnection,
};
use proton_core_db::{DBMigrationError, DBResult};

/// Extra initializer for the user database.
pub trait UserDatabaseInitializer {
    fn initialize(&self, conn: &mut SqliteConnection) -> Result<(), DBMigrationError>;
}

impl<F: Fn(&mut SqliteConnection) -> Result<(), DBMigrationError>> UserDatabaseInitializer for F {
    fn initialize(&self, conn: &mut SqliteConnection) -> Result<(), DBMigrationError> {
        (self)(conn)
    }
}

/// Contains all the relevant information to an initialize user session.
pub struct UserContext {
    session: Session,
    db_tracker: InProcessTrackerService,
    user_id: UserId,
}

impl UserContext {
    pub(crate) fn new(session: Session, db_pool: SqliteConnectionPool, id: UserId) -> Self {
        Self {
            session,
            db_tracker: InProcessTrackerService::new(db_pool),
            user_id: id,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_as<T: From<Session>>(&self) -> T {
        T::from(self.session.clone())
    }

    pub fn new_db_connection(&self) -> DBResult<TrackingConnection> {
        self.db_tracker.new_connection()
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
