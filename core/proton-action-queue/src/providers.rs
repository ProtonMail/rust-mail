use anyhow::Error as AnyhowError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::session::Session;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionProviderError {
    #[error("{0}")]
    Api(#[source] ApiServiceError),
    #[error("{0}")]
    Client(#[source] AnyhowError),
    #[error("{0}")]
    Other(#[source] AnyhowError),
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

#[derive(Debug, Error)]
pub enum SqliteConnectionProviderError {
    #[error("DB: {0}")]
    DB(
        #[source]
        #[from]
        proton_sqlite3::rusqlite::Error,
    ),
    #[error("{0}")]
    Other(#[source] AnyhowError),
}
