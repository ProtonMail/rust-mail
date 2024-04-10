use proton_api_core::exports::anyhow;
use proton_api_core::exports::thiserror;
use proton_api_core::Session;

#[derive(Debug, thiserror::Error)]
pub enum SessionProviderError {
    #[error("{0}")]
    Http(#[source] proton_api_core::http::RequestError),
    #[error("{0}")]
    Client(#[source] anyhow::Error),
    #[error("{0}")]
    Other(#[source] anyhow::Error),
}

/// Provide a session for remote state execution.
pub trait SessionProvider: Send + Sync {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError>;
}

pub struct AlwaysErrorSessionProvider {}

impl SessionProvider for AlwaysErrorSessionProvider {
    fn retrieve_session(&self) -> Result<Session, SessionProviderError> {
        Err(SessionProviderError::Other(anyhow::anyhow!("failure")))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SqliteConnectionProviderError {
    #[error("DB: {0}")]
    DB(
        #[source]
        #[from]
        proton_sqlite3::rusqlite::Error,
    ),
    #[error("{0}")]
    Other(#[source] anyhow::Error),
}

/// Provider of SQL connections.
pub trait SqlConnectionProvider: Send + Sync {
    fn new_connection(
        &self,
    ) -> Result<proton_sqlite3::TrackingConnection, SqliteConnectionProviderError>;
}

/// Default provider which directly interacts with [`proton_sqlite3::InProcessTrackerService`].
pub struct DefaultSqlConnectionProvider(proton_sqlite3::InProcessTrackerService);
impl DefaultSqlConnectionProvider {
    pub fn new(tracker: proton_sqlite3::InProcessTrackerService) -> Self {
        Self(tracker)
    }
}

impl SqlConnectionProvider for DefaultSqlConnectionProvider {
    fn new_connection(
        &self,
    ) -> Result<proton_sqlite3::TrackingConnection, SqliteConnectionProviderError> {
        let conn = self.0.new_connection()?;
        Ok(conn)
    }
}
