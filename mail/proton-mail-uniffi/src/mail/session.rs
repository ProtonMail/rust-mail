use crate::core::datatypes::ApiConfig;
use crate::core::{
    FFIKeyChain, FFINetworkStatusChanged, NetworkStatusChanged, StoredAccountState, StoredSession,
    StoredSessionState,
};
use crate::core::{OSKeyChain, StoredAccount};
use crate::errors::login_flow::UserLoginFlowArcLoginFlowResult;
use crate::mail::logging::init_log;
use crate::mail::{LoginFlow, MailUserSession};
use crate::{async_runtime, uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use anyhow::anyhow;
use proton_action_queue::action::Action;
use proton_action_queue::queue::{
    ActionError as QueueActionError, Error as QueueError, QueuedError,
};
use proton_api_core::login::LoginError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Proton;
use proton_core_common::cache::CacheError;
use proton_core_common::db::account::{CoreAccount, CoreSession, SessionEncryptionKey};
use proton_core_common::db::ChangeReceiver;
use proton_core_common::ContactError;
use proton_event_loop::EventLoopError;
use proton_mail_common::actions::ActionError;
use proton_mail_common::db::DBMigrationError;
use proton_mail_common::errors::login_flow::UserLoginFlowError as RealUserLoginFlowError;
use proton_mail_common::MailContextError;
use proton_mail_common::{AppError, MailContext};
use stash::stash::{Stash, StashError};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinError;
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
    #[error("Login Error: {0}")]
    Login(#[from] LoginError),
    #[error("API Error: {0}")]
    Api(#[from] ApiServiceError),
    #[error("Cache Error: {0}")]
    CacheError(#[from] CacheError),
    #[error("Problem with loading contact: {0}")]
    ContactError(#[from] ContactError),
    #[error("{0}")]
    Other(anyhow::Error),
}

impl From<JoinError> for MailSessionError {
    fn from(value: JoinError) -> Self {
        Self::Other(anyhow::Error::new(value))
    }
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

// #[uniffi::export]
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
    pub fn create(
        params: MailSessionParams,
        key_chain: Box<dyn OSKeyChain>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> MailSessionResult<Arc<Self>> {
        async_runtime().block_on(async move {
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

            debug!("Creating Context");
            let mail_ctx = MailContext::new(
                session_path,
                user_path,
                mail_cache_path,
                params.mail_cache_size,
                Arc::from(FFIKeyChain::from(key_chain)),
                api_env_config.into(),
                network_callback.map(|v| -> Box<dyn proton_core_common::NetworkStatusChanged> {
                    Box::new(FFINetworkStatusChanged::from(v))
                }),
            )
            .await?;
            Ok(Arc::new(Self { ctx: mail_ctx }))
        })
    }

    /// Start new login flow.
    pub async fn new_login_flow(&self) -> UserLoginFlowArcLoginFlowResult {
        let ctx = self.ctx.clone();
        uniffi_async::<_, RealUserLoginFlowError, _>(async move {
            let flow = ctx
                .new_login_flow()
                .await
                .map_err(RealUserLoginFlowError::from)?;
            Ok(LoginFlow::new(flow, ctx))
        })
        .await
        .into()
    }

    /// Resume an existing login flow.
    pub async fn resume_login_flow(
        &self,
        user_id: String,
        session_id: String,
    ) -> UserLoginFlowArcLoginFlowResult {
        let ctx = self.ctx.clone();

        uniffi_async::<_, RealUserLoginFlowError, _>(async move {
            let flow = ctx
                .resume_login_flow(user_id.into(), session_id.into())
                .await
                .map_err(RealUserLoginFlowError::from)?;

            Ok(LoginFlow::new(flow, ctx))
        })
        .await
        .into()
    }

    /// Create an user context from a stored session.
    pub fn user_context_from_session(
        &self,
        session: Arc<StoredSession>,
    ) -> MailSessionResult<Arc<MailUserSession>> {
        async_runtime().block_on(async move {
            let ctx = self
                .ctx
                .user_context_from_session(session.session())
                .await?;

            Ok(MailUserSession::new(ctx))
        })
    }

    /// Get all available accounts.
    ///
    /// An account is an entity representing a Proton account known to the system.
    /// When a user first authenticates via the login flow, a new account is created,
    /// and all subsequent sessions are associated with that account.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the accounts from the db.
    pub async fn get_accounts(&self) -> MailSessionResult<Vec<Arc<StoredAccount>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            for account in ctx.get_accounts().await? {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                };
            }

            Ok(accounts)
        })
        .await
    }

    /// Watch the accounts for changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_accounts(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> MailSessionResult<WatchedAccounts> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            let (initial, rx) = ctx.watch_accounts().await?;

            for account in initial {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                };
            }

            Ok(WatchedAccounts::new(accounts, rx, callback))
        })
        .await
    }

    /// Get a single account by its remote (user) ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account(
        &self,
        user_id: String,
    ) -> MailSessionResult<Option<Arc<StoredAccount>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(account) = ctx.get_account(user_id.into()).await? else {
                return Ok(None);
            };

            let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? else {
                return Ok(None);
            };

            Ok(Some(StoredAccount::new(account, state)))
        })
        .await
    }

    /// Get all API sessions associated with a given account.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_sessions(
        &self,
        account: Arc<StoredAccount>,
    ) -> MailSessionResult<Vec<Arc<StoredSession>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let account = account.account();

            let mut sessions = Vec::new();

            for session in ctx.get_sessions(account.remote_id.clone()).await? {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Ok(sessions)
        })
        .await
    }

    /// Watch an account's API sessions for changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_sessions(
        &self,
        account: Arc<StoredAccount>,
        callback: Box<dyn LiveQueryCallback>,
    ) -> MailSessionResult<WatchedSessions> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_sessions(account.user_id().into()).await?;

            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Ok(WatchedSessions::new(sessions, rx, callback))
        })
        .await
    }

    /// Get a single API session by its associated account and session ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session(
        &self,
        session_id: String,
    ) -> MailSessionResult<Option<Arc<StoredSession>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(session) = ctx.get_session(session_id.into()).await? else {
                return Ok(None);
            };

            let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? else {
                return Ok(None);
            };

            Ok(Some(StoredSession::new(session, state)))
        })
        .await
    }

    /// Get the login state of an account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account_state(
        &self,
        user_id: String,
    ) -> MailSessionResult<Option<StoredAccountState>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let state = ctx
                .get_account_state(user_id.into())
                .await?
                .map(StoredAccountState::from);

            Ok(state)
        })
        .await
    }

    /// Get the login state of a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session_state(
        &self,
        session_id: String,
    ) -> MailSessionResult<Option<StoredSessionState>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let state = ctx
                .get_session_state(session_id.into())
                .await?
                .map(StoredSessionState::from);

            Ok(state)
        })
        .await
    }

    /// Get the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_primary_account(&self) -> MailSessionResult<Option<Arc<StoredAccount>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(account) = ctx.get_primary_account().await? else {
                return Ok(None);
            };

            let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? else {
                return Ok(None);
            };

            Ok(Some(StoredAccount::new(account, state)))
        })
        .await
    }

    /// Set the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the account is not found.
    pub async fn set_primary_account(&self, user_id: String) -> MailSessionResult<()> {
        let ctx = self.ctx.clone();

        uniffi_async(async move { Ok(ctx.set_primary_account(user_id.into()).await?) }).await
    }

    /// Removes an account and all associated sessions and data.
    ///
    /// # Errors
    /// Returns error if data can not be removed or the db operation failed.
    pub async fn delete_account(&self, account: Arc<StoredAccount>) -> MailSessionResult<()> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let account = account.account();
            let user_id = account.remote_id.clone();

            Ok(ctx.delete_account(user_id).await?)
        })
        .await
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

#[uniffi::export]
impl MailSession {
    /// A blocking form of `get_accounts`.
    pub fn get_accounts_blocking(&self) -> MailSessionResult<Vec<Arc<StoredAccount>>> {
        async_runtime().block_on(self.get_accounts())
    }

    /// A blocking form of `get_account`.
    pub fn get_account_blocking(
        &self,
        user_id: String,
    ) -> MailSessionResult<Option<Arc<StoredAccount>>> {
        async_runtime().block_on(self.get_account(user_id))
    }

    /// A blocking form of `get_sessions`.
    pub fn get_sessions_blocking(
        &self,
        account: Arc<StoredAccount>,
    ) -> MailSessionResult<Vec<Arc<StoredSession>>> {
        async_runtime().block_on(self.get_sessions(account))
    }

    /// A blocking form of `get_session`.
    pub fn get_session_blocking(
        &self,
        session_id: String,
    ) -> MailSessionResult<Option<Arc<StoredSession>>> {
        async_runtime().block_on(self.get_session(session_id))
    }

    /// A blocking form of `get_account_state`.
    pub fn get_account_state_blocking(
        &self,
        user_id: String,
    ) -> MailSessionResult<Option<StoredAccountState>> {
        async_runtime().block_on(self.get_account_state(user_id))
    }

    /// A blocking form of `get_session_state`.
    pub fn get_session_state_blocking(
        &self,
        session_id: String,
    ) -> MailSessionResult<Option<StoredSessionState>> {
        async_runtime().block_on(self.get_session_state(session_id))
    }

    /// A blocking form of `get_primary_account`.
    pub fn get_primary_account_blocking(&self) -> MailSessionResult<Option<Arc<StoredAccount>>> {
        async_runtime().block_on(self.get_primary_account())
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

    /// Get the session database connection.
    #[must_use]
    pub fn session_stash(&self) -> &Stash {
        self.ctx.session_stash()
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
            MailContextError::Login(e) => Self::Login(e),
            MailContextError::Api(e) => Self::Api(e),
            MailContextError::CacheError(e) => Self::CacheError(e),
            MailContextError::ContactError(e) => Self::ContactError(e),
            MailContextError::Other(err) => Self::Other(err),
        }
    }
}

impl<T> From<QueueActionError<T>> for MailSessionError
where
    T: Action<Error = ActionError>,
{
    fn from(value: QueueActionError<T>) -> Self {
        match value {
            QueueActionError::Action(error) => Self::Action(error),
            QueueActionError::Queue(error) => Self::ActionQueue(error),
        }
    }
}

/// Data for watched sessions.
#[derive(uniffi::Record)]
pub struct WatchedAccounts {
    /// The accounts.
    pub accounts: Vec<Arc<StoredAccount>>,

    /// The handle to stop watching the accounts.
    pub handle: Arc<WatchHandle>,
}

impl WatchedAccounts {
    fn new(
        accounts: Vec<Arc<StoredAccount>>,
        receiver: ChangeReceiver<CoreAccount>,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedAccounts {
        let handle = watch_channel(receiver, callback);

        WatchedAccounts { accounts, handle }
    }
}

/// Data for watched sessions.
#[derive(uniffi::Record)]
pub struct WatchedSessions {
    /// The sessions.
    pub sessions: Vec<Arc<StoredSession>>,

    /// The handle to stop watching the sessions.
    pub handle: Arc<WatchHandle>,
}

impl WatchedSessions {
    fn new(
        sessions: Vec<Arc<StoredSession>>,
        receiver: ChangeReceiver<CoreSession>,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedSessions {
        let handle = watch_channel(receiver, callback);

        WatchedSessions { sessions, handle }
    }
}
