use crate::core::datatypes::ApiConfig;
use crate::core::{FFIKeyChain, FFINetworkStatusChanged, NetworkStatusChanged};
use crate::core::{OSKeyChain, StoredSession};
use crate::mail::logging::init_log;
use crate::mail::{LoginFlow, MailUserSession};
use anyhow::anyhow;
use proton_action_queue::queue::{Error as QueueError, QueuedError};
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Proton;
use proton_core_common::cache::CacheError;
use proton_core_common::db::session::SessionEncryptionKey;
use proton_event_loop::EventLoopError;
use proton_mail_common::actions::ActionError;
use proton_mail_common::db::DBMigrationError;
use proton_mail_common::MailContextError;
use proton_mail_common::{AppError, MailContext};
use stash::stash::{Stash, StashError};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

/// Mail context is the entry point for the application. It contains important state such as
/// database connection pools and the async runtime for rust.
///
/// # Lifetime
/// This object needs to be kept alive for the entire duration of the application.
///
#[derive(uniffi::Object)]
pub struct MailSession {
    ctx: MailContext,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailSessionError {
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(String),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] DBMigrationError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] QueueError),
    #[error("Action: {0}")]
    Action(ActionError),
    #[error("QueuedAction: {0}")]
    QueuedAction(#[from] QueuedError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(anyhow::Error),
    #[error("Invalid mode: '{0}'")]
    InvalidImageMode(String),
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
pub type MailSessionResult<T> = Result<T, MailSessionError>;

/// Configuration parameters for the [`MailSession`]
#[derive(uniffi::Record)]
pub struct MailSessionParams {
    /// Directory where the session database should be stored.
    pub session_dir: String,
    /// Directory where the user databases should be stored.
    pub user_dir: String,
    /// Directory where the mail cache should be stored.
    pub mail_cache_dir: String,
    /// Size of the mail cache.
    pub mail_cache_size: u32,
    /// Directory where the logs should be stored.
    pub log_dir: String,
    /// Whether to enable debug and trace logs.
    pub log_debug: bool,
    /// API Environment configuration.
    pub api_env_config: Option<ApiConfig>,
}

#[uniffi::export]
impl MailSession {
    // NOTE: Callbacks can not be stored in record types, which is why they are still in the
    // constructor.
    /// Create a new mail session.
    ///
    /// # Parameters
    ///
    /// * `params`: See [`MailSessionParams`] for parameter details.
    /// * `key_chain`: Keychain implementation.
    /// * `network_callback`: Optional network status changes callback.
    ///
    /// # Panics
    ///
    /// Panics if the API URL is invalid. In this situation we cannot proceed.
    ///
    /// TODO: An error type needs to be added for this later.
    ///
    #[uniffi::constructor]
    pub async fn create(
        params: MailSessionParams,
        key_chain: Box<dyn OSKeyChain>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> MailSessionResult<Arc<Self>> {
        let mut log_path = PathBuf::from(params.log_dir);
        std::fs::create_dir_all(&log_path)?;
        log_path.push("proton-mail-uniffi.log");

        init_log(&log_path, params.log_debug)?;

        let session_path = PathBuf::from(params.session_dir);
        let user_path = PathBuf::from(params.user_dir);
        let mail_cache_path = PathBuf::from(params.mail_cache_dir);

        // create directories.
        debug!("Creating directories");
        std::fs::create_dir_all(&session_path)?;
        std::fs::create_dir_all(&user_path)?;
        std::fs::create_dir_all(&mail_cache_path)?;

        // Generate session key;
        debug!("Checking keychain");
        if key_chain
            .get()
            .map_err(|e| MailSessionError::KeyChain(e.to_string()))?
            .is_none()
        {
            debug!("Key chain has no key, generating");
            let key = SessionEncryptionKey::random();
            key_chain.store(key.to_base64()).map_err(|e| {
                tracing::error!("Failed to store key in keychain");
                MailSessionError::KeyChain(e.to_string())
            })?;
        }

        // Creating client.
        let api_env_config = params.api_env_config.unwrap_or_default();
        let api_url = api_env_config.base_url.parse().expect("Invalid API URL");

        debug!("Creating Context");
        let mail_ctx = MailContext::new(
            session_path,
            user_path,
            mail_cache_path,
            params.mail_cache_size,
            Arc::from(FFIKeyChain::from(key_chain)),
            api_url,
            network_callback.map(|v| -> Box<dyn proton_core_common::NetworkStatusChanged> {
                Box::new(FFINetworkStatusChanged::from(v))
            }),
        )
        .await?;
        Ok(Arc::new(Self { ctx: mail_ctx }))
    }

    /// Start new login flow.
    pub async fn new_login_flow(&self) -> MailSessionResult<Arc<LoginFlow>> {
        let flow = self.ctx.new_login_flow().await?;
        Ok(LoginFlow::new(flow, self.ctx.clone()))
    }

    /// Return the list of active session.
    ///
    /// # Errors
    /// Returns error if the db query failed.
    pub async fn stored_sessions(&self) -> MailSessionResult<Vec<Arc<StoredSession>>> {
        let sessions = self.ctx.sessions().await?;
        Ok(sessions
            .into_iter()
            .map(StoredSession::new)
            .collect::<Vec<_>>())
    }

    /// Create an user context from a stored session.
    pub async fn user_context_from_session(
        &self,
        session: &StoredSession,
    ) -> MailSessionResult<Arc<MailUserSession>> {
        let ctx = self
            .ctx
            .user_context_from_session(session.encrypted_session())
            .await?;
        Ok(MailUserSession::new(ctx))
    }

    /// Removes a user session and deletes all associated data.
    ///
    /// # Errors
    /// Returns error if data can not be removed or the db operation failed.
    pub async fn delete_session(&self, session: &StoredSession) -> MailSessionResult<()> {
        Ok(self.ctx.delete_session(session.encrypted_session()).await?)
    }

    /// Check whether the network is connected/online.
    #[must_use]
    pub fn is_network_connected(&self) -> bool {
        self.ctx.is_network_connected()
    }

    /// Externally notify the context that the network connection has changed.
    pub fn set_network_connected(&self, online: bool) {
        self.ctx.set_network_connected(online);
    }
}

impl MailSession {
    /// Get the mail context.
    #[must_use]
    pub fn ctx(&self) -> &MailContext {
        &self.ctx
    }

    /// Get the API service.
    #[must_use]
    pub fn api(&self) -> &Proton {
        self.ctx.api()
    }

    /// Get the database connection.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        self.ctx.stash()
    }
}

impl From<MailContextError> for MailSessionError {
    fn from(value: MailContextError) -> Self {
        match value {
            MailContextError::Crypto => Self::Crypto,
            MailContextError::KeyChain(k) => Self::KeyChain(k.to_string()),
            MailContextError::IO(io) => Self::IO(io),
            MailContextError::DBMigration(err) => Self::DBMigration(err),
            MailContextError::KeyChainHasNoKey => Self::KeyChainHasNoKey,
            MailContextError::EventLoop(err) => Self::EventLoop(err),
            MailContextError::ActionQueue(e) => Self::ActionQueue(e),
            MailContextError::Action(e) => Self::Action(e),
            MailContextError::QueuedAction(e) => Self::QueuedAction(e),
            MailContextError::PGPKeyAccess(e) => Self::PGPKeyAccess(anyhow!("{e}")),
            MailContextError::App(e) => Self::App(e),
            MailContextError::Stash(e) => Self::Stash(e),
            MailContextError::Api(e) => Self::Api(e),
            MailContextError::CacheError(e) => Self::CacheError(e),
            MailContextError::Other(err) => Self::Other(err),
        }
    }
}
