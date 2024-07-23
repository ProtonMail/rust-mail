use crate::db::DBMigrationError;
use crate::{AppError, MailUserContext};
use futures::executor::block_on;
use proton_api_core::login::Flow;
use proton_api_core::service::ApiServiceError;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::db::session::EncryptedUserSession;
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::{Context, CoreContextError, KeyHandlingError};
use proton_core_common::{NetworkStatusChanged, UserDatabaseInitializer};
use proton_event_loop::EventLoopError;
use stash::stash::{Stash, StashError};
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

/// Errors that may occur while interacting with a MailContext.
#[derive(Debug, thiserror::Error)]
pub enum MailContextError {
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
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::QueueError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(KeyHandlingError),
    #[error("Stash Error: {0}")]
    App(#[from] AppError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("API Error: {0}")]
    Api(#[from] ApiServiceError),
    #[error("{0}")]
    Other(anyhow::Error),
}

impl From<CoreContextError> for MailContextError {
    fn from(value: CoreContextError) -> Self {
        match value {
            CoreContextError::Api(err) => MailContextError::Api(err),
            CoreContextError::Crypto => MailContextError::Crypto,
            CoreContextError::KeyChain(err) => MailContextError::KeyChain(err),
            CoreContextError::IO(err) => MailContextError::IO(err),
            CoreContextError::DBMigration(err) => MailContextError::DBMigration(err),
            CoreContextError::KeyChainHasNoKey => MailContextError::KeyChainHasNoKey,
            CoreContextError::Other(err) => MailContextError::Other(err),
            CoreContextError::PGPKeyAccess(err) => MailContextError::PGPKeyAccess(err),
            CoreContextError::Stash(err) => MailContextError::Stash(err),
        }
    }
}

pub type MailContextResult<T> = Result<T, MailContextError>;

#[derive(Clone)]
pub struct MailContext {
    core_context: Arc<Context>,
    // TODO: cleanup after Dan's refactor.
    mail_cache_path: PathBuf,
}

impl MailContext {
    pub async fn new(
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        mail_cache_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        api_url: Url,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> Result<Self, MailContextError> {
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(MailUserDatabaseInitializer {})];
        let core_context = Context::new(
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            api_url,
            network_callback,
        )
        .await?;

        Ok(Self {
            core_context,
            mail_cache_path: mail_cache_path.into(),
        })
    }

    pub fn new_login_flow(&self) -> MailContextResult<Flow> {
        let f = self.core_context.new_login_flow()?;
        Ok(f)
    }

    /// Create a new context from a login flow.
    ///
    /// # Errors
    /// Returns error if the flow is in an invalid state or there was an issue initializing
    /// the user database.
    pub async fn user_context_from_login_flow(
        &self,
        login_flow: &Flow,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self
            .core_context
            .user_context_from_login_flow(login_flow)
            .await?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }

    /// Create a new context from an existing session.
    ///
    /// # Errors
    /// Returns error if we failed to decrypt the user session or access the user database.
    pub async fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self.core_context.user_context_from_session(session).await?;
        Ok(MailUserContext::new(self.clone(), ctx))
    }
    /// Return the list of active session.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub async fn sessions(&self) -> MailContextResult<Vec<EncryptedUserSession>> {
        Ok(self.core_context.get_sessions().await?)
    }

    /// Removes a user session and deletes all associated data.
    ///
    /// # Errors
    /// Returns error if data can not be removed or the db operation failed.
    pub async fn delete_session(&self, session: &EncryptedUserSession) -> MailContextResult<()> {
        Ok(self.core_context.delete_session(session).await?)
    }
    pub fn set_network_connected(&self, value: bool) {
        self.core_context.set_network_connected(value)
    }

    pub fn is_network_connected(&self) -> bool {
        self.core_context.is_network_corrected()
    }

    /// Path where mail content should be cached for user with `user_id`.
    pub fn mail_cache_path(&self, user_id: &RemoteId) -> PathBuf {
        self.mail_cache_path.join(user_id.to_string())
    }
}

struct MailUserDatabaseInitializer {}

impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    fn initialize(&self, stash: &Stash) -> Result<(), DBMigrationError> {
        block_on(async {
            crate::db::migrations::migrate_db(stash).await?;
            proton_action_queue::ActionStore::init_tables(stash).await?;
            Ok(())
        })
    }
}
