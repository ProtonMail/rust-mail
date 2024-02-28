use crate::state::DataLoadError;
use proton_api_mail::proton_api_core::domain::User;
use proton_api_mail::proton_api_core::exports::proton_sqlite3::{InProcessTrackerService, SqliteConnectionPool, SqliteMode};
use proton_api_mail::proton_api_core::Session;
use proton_api_mail::MailSession;
use proton_mail_db::MailSqliteConnectionPool;
use std::path::PathBuf;

pub struct UserState {
    pub user: User,
    pub session: MailSession,
    pub db_pool: MailSqliteConnectionPool,
}

impl UserState {
    pub async fn new(session: Session, mut db_path: PathBuf) -> Result<Self, DataLoadError> {
        let user = session.get_user().await?;
        db_path.push(user.id.as_ref());
        let connection_pool = SqliteConnectionPool::new(SqliteMode::File(db_path), false);
        let tracker_service= InProcessTrackerService::new(connection_pool);
        let db_pool = MailSqliteConnectionPool::new(tracker_service)?;
        Ok(Self {
            user,
            session: MailSession::new(session),
            db_pool,
        })
    }
}
