use crate::actions::ActionError;
use crate::{AppError, MailUserContext};
use futures::executor::block_on;
use proton_action_queue::action::Action;
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedError};
use proton_api_core::login::Flow;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Proton;
use proton_core_common::cache::CacheError;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::db::session::EncryptedUserSession;
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::{Context, CoreContextError, KeyHandlingError};
use proton_core_common::{NetworkStatusChanged, UserDatabaseInitializer};
use proton_event_loop::EventLoopError;
use proton_sqlite3::MigratorError;
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
    DBMigration(#[from] MigratorError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::queue::Error),
    #[error("Action: {0}")]
    Action(ActionError),
    #[error("QueuedAction: {0}")]
    QueuedAction(#[from] QueuedError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(KeyHandlingError),
    #[error("App Error: {0}")]
    App(#[from] AppError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("API Error: {0}")]
    Api(#[from] ApiServiceError),
    #[error("Cache Error: {0}")]
    CacheError(#[from] CacheError),
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
            CoreContextError::CacheError(err) => MailContextError::CacheError(err),
        }
    }
}
pub type MailContextResult<T> = Result<T, MailContextError>;

impl<T: Action<Error = ActionError>> From<QueueActionError<T>> for MailContextError {
    fn from(value: QueueActionError<T>) -> Self {
        match value {
            QueueActionError::Action(e) => Self::Action(e),
            QueueActionError::Queue(e) => Self::ActionQueue(e),
        }
    }
}
#[derive(Clone)]
pub struct MailContext {
    core_context: Arc<Context>,
    // TODO: cleanup after Dan's refactor.
    mail_cache_path: PathBuf,
    pub(crate) mail_cache_size: u32,
}

impl MailContext {
    pub async fn new(
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        mail_cache_path: impl Into<PathBuf>,
        mail_cache_size: u32,
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
            mail_cache_size,
        })
    }

    pub async fn new_login_flow(&self) -> MailContextResult<Flow> {
        let f = self.core_context.new_login_flow().await?;
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
            .user_context_from_login_flow(
                login_flow,
                self.mail_cache_path.clone(),
                self.mail_cache_size,
            )
            .await?;
        MailUserContext::new(self.clone(), ctx)
    }

    /// Create a new context from an existing session.
    ///
    /// # Errors
    /// Returns error if we failed to decrypt the user session or access the user database.
    pub async fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self
            .core_context
            .user_context_from_session(session, self.mail_cache_path.clone(), self.mail_cache_size)
            .await?;
        Ok(MailUserContext::new(self.clone(), ctx).await)
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

    /// Get the core context.
    pub fn core_context(&self) -> &Arc<Context> {
        &self.core_context
    }

    /// Get the API service.
    pub fn api(&self) -> &Proton {
        self.core_context.api()
    }

    /// Get the database connection.
    pub fn stash(&self) -> &Stash {
        self.core_context.stash()
    }
}

struct MailUserDatabaseInitializer {}

impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    fn initialize(&self, stash: &Stash) -> Result<(), MigratorError> {
        block_on(async {
            crate::db::migrations::migrate_db(stash).await?;
            Ok(())
        })
    }
}
