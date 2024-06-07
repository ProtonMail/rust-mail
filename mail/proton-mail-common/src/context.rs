use crate::db::DBMigrationError;
use crate::MailUserContext;
use proton_api_mail::domain::AddressDomainLogoError;
use proton_api_mail::proton_api_core::exports::{anyhow, thiserror};
use proton_api_mail::proton_api_core::http::{Client, RequestError};
use proton_api_mail::proton_api_core::login::Flow;
use proton_async::runtime::MultiThreaded;
use proton_core_common::db::EncryptedUserSession;
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::{Context, CoreContextError, KeyHandlingError};
use proton_core_common::{CoreSessionCallback, NetworkStatusChanged, UserDatabaseInitializer};
use proton_event_loop::EventLoopError;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Errors that may occur while interacting with a MailContext.
#[derive(Debug, thiserror::Error)]
pub enum MailContextError {
    #[error("Database Error: {0}")]
    DB(#[from] crate::db::DBError),
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
    Http(#[from] RequestError),
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::QueueError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(KeyHandlingError),
    #[error("Creating AddressDomainLogoDetails failed with error: '{0}'")]
    AddressDomainLogoError(#[from] AddressDomainLogoError),
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
            CoreContextError::PGPKeyAccess(err) => MailContextError::PGPKeyAccess(err),
        }
    }
}

pub type MailContextResult<T> = Result<T, MailContextError>;

#[derive(Clone)]
pub struct MailContext {
    core_context: Context,
    // TODO: cleanup after Dan's refactor.
    mail_cache_path: PathBuf,
}

impl MailContext {
    pub fn new(
        async_runtime: MultiThreaded,
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        mail_cache_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        client: Client,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> Result<Self, MailContextError> {
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(MailUserDatabaseInitializer {})];
        let core_context = Context::new(
            async_runtime,
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            client,
            network_callback,
        )?;

        Ok(Self {
            core_context,
            mail_cache_path: mail_cache_path.into(),
        })
    }

    pub fn new_login_flow(
        &self,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> MailContextResult<Flow> {
        let f = self.core_context.new_login_flow(cb)?;
        Ok(f)
    }

    /// Create a new context from a login flow.
    ///
    /// # Errors
    /// Returns error if the flow is in an invalid state or there was an issue initializing
    /// the user database.
    pub fn user_context_from_login_flow(
        &self,
        login_flow: &Flow,
    ) -> MailContextResult<MailUserContext> {
        let ctx = self.core_context.user_context_from_login_flow(login_flow)?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }

    /// Create a new context from an existing session.
    ///
    /// # Errors
    /// Returns error if we failed to decrypt the user session or access the user database.
    pub fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> MailContextResult<MailUserContext> {
        let ctx = self.core_context.user_context_from_session(session, cb)?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }
    /// Return the list of active session.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub fn sessions(&self) -> MailContextResult<Vec<EncryptedUserSession>> {
        Ok(self.core_context.get_sessions()?)
    }

    /// Removes a user session and deletes all associated data.
    ///
    /// # Errors
    /// Returns error if data can not be removed or the db operation failed.
    pub fn delete_session(&self, session: &EncryptedUserSession) -> MailContextResult<()> {
        Ok(self.core_context.delete_session(session)?)
    }
    pub fn set_network_connected(&self, value: bool) {
        self.core_context.set_network_connected(value)
    }

    pub fn is_network_connected(&self) -> bool {
        self.core_context.is_network_corrected()
    }

    pub fn async_runtime(&self) -> &MultiThreaded {
        self.core_context.async_runtime()
    }

    /// Path where mail content should be cached.
    pub fn mail_cache_path(&self) -> &Path {
        &self.mail_cache_path
    }
}

struct MailUserDatabaseInitializer {}

impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    fn initialize(
        &self,
        conn: &mut crate::db::proton_sqlite3::SqliteConnection,
    ) -> Result<(), DBMigrationError> {
        crate::db::migrations::migrate_db(conn)?;
        proton_action_queue::ActionStore::init_tables(conn)?;
        Ok(())
    }
}
