use crate::MailUserContext;
use proton_api_mail::proton_api_core::exports::{anyhow, thiserror};
use proton_api_mail::proton_api_core::http::HttpRequestError;
use proton_api_mail::proton_api_core::login::LoginFlow;
use proton_async::runtime::MTRuntime;
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::proton_core_db::EncryptedUserSession;
use proton_core_common::{CoreContext, CoreContextError};
use proton_core_common::{CoreSessionCallback, NetworkStatusChanged, UserDatabaseInitializer};
use proton_event_loop::EventLoopError;
use proton_mail_db::DBMigrationError;
use std::path::PathBuf;
use std::sync::Arc;

/// Errors that may occur while interacting with a MailContext.
#[derive(Debug, thiserror::Error)]
pub enum MailContextError {
    #[error("Database Error: {0}")]
    DB(#[from] proton_mail_db::DBError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] DBMigrationError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("HTTP Error: {0}")]
    Http(#[from] HttpRequestError),
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("{0}")]
    Other(anyhow::Error),
}

impl From<CoreContextError> for MailContextError {
    fn from(value: CoreContextError) -> Self {
        match value {
            CoreContextError::DB(db) => MailContextError::DB(db),
            CoreContextError::Crypto => MailContextError::Crypto,
            CoreContextError::KeyChain(err) => MailContextError::KeyChain(err),
            CoreContextError::IO(err) => MailContextError::IO(err),
            CoreContextError::DBMigration(err) => MailContextError::DBMigration(err),
            CoreContextError::KeyChainHasNoKey => MailContextError::KeyChainHasNoKey,
            CoreContextError::Other(err) => MailContextError::Other(err),
            CoreContextError::Http(err) => MailContextError::Http(err),
        }
    }
}

pub type MailContextResult<T> = Result<T, MailContextError>;

#[derive(Clone)]
pub struct MailContext {
    core_context: CoreContext,
}

impl MailContext {
    pub fn new(
        async_runtime: MTRuntime,
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> Result<Self, MailContextError> {
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(MailUserDatabaseInitializer {})];
        let core_context = CoreContext::new(
            async_runtime,
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            network_callback,
        )?;

        Ok(Self { core_context })
    }

    pub fn new_login_flow(
        &self,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> MailContextResult<LoginFlow> {
        let f = self.core_context.new_login_flow(cb)?;
        Ok(f)
    }

    pub fn user_context_from_login_flow(
        &self,
        login_flow: &LoginFlow,
    ) -> MailContextResult<MailUserContext> {
        let ctx = self.core_context.user_context_from_login_flow(login_flow)?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }

    pub fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> MailContextResult<MailUserContext> {
        let ctx = self.core_context.user_context_from_session(session, cb)?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }
    pub fn get_sessions(&self) -> MailContextResult<Vec<EncryptedUserSession>> {
        let s = self.core_context.get_sessions()?;
        Ok(s)
    }
    pub fn set_network_connected(&self, value: bool) {
        self.core_context.set_network_connected(value)
    }

    pub fn is_network_connected(&self) -> bool {
        self.core_context.is_network_corrected()
    }

    pub fn async_runtime(&self) -> &MTRuntime {
        self.core_context.async_runtime()
    }
}

struct MailUserDatabaseInitializer {}

impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    fn initialize(
        &self,
        conn: &mut proton_mail_db::proton_sqlite3::SqliteConnection,
    ) -> Result<(), DBMigrationError> {
        proton_mail_db::migrations::migrate_db(conn)?;
        Ok(())
    }
}
