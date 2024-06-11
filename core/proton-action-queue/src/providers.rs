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
