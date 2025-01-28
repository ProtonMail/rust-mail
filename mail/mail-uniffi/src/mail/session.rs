use crate::core::datatypes::ApiConfig;
use crate::core::{
    FFIKeyChain, FFINetworkStatusChanged, NetworkStatusChanged, StoredAccountState, StoredSession,
    StoredSessionState,
};
use crate::core::{OSKeyChain, StoredAccount};
use crate::errors::{LoginError, UserSessionError, VoidSessionResult};
use crate::mail::logging::init_log;
use crate::mail::{LoginFlow, MailUserSession};
use crate::{async_runtime, uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use crate::{watch_channel_async, AsyncLiveQueryCallback};
use proton_core_common::db::account::SessionEncryptionKey;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::MailContext;
use stash::stash::{Stash, WatcherHandle};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;
use tracing_appender::non_blocking::WorkerGuard;

/// Mail context is the entry point for the application. It contains important state such as
/// database connection pools and the async runtime for rust.
///
/// # Lifetime
/// This object needs to be kept alive for the entire duration of the application.
///
#[derive(uniffi::Object)]
pub struct MailSession {
    ctx: Arc<MailContext>,
    _log_guard: WorkerGuard,
}

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
    pub mail_cache_size: u64,
    /// Directory where the logs should be stored.
    pub log_dir: String,
    /// Whether to enable debug and trace logs.
    pub log_debug: bool,
    /// API Environment configuration.
    pub api_env_config: Option<ApiConfig>,
}

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
#[must_use]
#[proton_uniffi_macros::export_result]
pub fn create_mail_session(
    params: MailSessionParams,
    key_chain: Box<dyn OSKeyChain>,
    network_callback: Option<Box<dyn NetworkStatusChanged>>,
) -> Result<Arc<MailSession>, UserSessionError> {
    async_runtime()
        .block_on(async move {
            let mut log_path = PathBuf::from(params.log_dir);
            std::fs::create_dir_all(&log_path)?;
            log_path.push("proton-mail-uniffi.log");

            let log_guard = init_log(&log_path, params.log_debug)?;

            let session_path = PathBuf::from(params.session_dir);
            let user_path = PathBuf::from(params.user_dir);
            let cache_path = PathBuf::from(params.mail_cache_dir);
            let mail_cache_path = cache_path.join("mail-cache");
            let core_cache_path = cache_path.join("core-cache");

            // create directories.
            debug!("Creating directories");
            std::fs::create_dir_all(&session_path)?;
            std::fs::create_dir_all(&user_path)?;
            std::fs::create_dir_all(&mail_cache_path)?;
            std::fs::create_dir_all(&core_cache_path)?;

            // Generate session key;
            debug!("Checking keychain");
            if key_chain.get().map_err(|_| Unexpected::Os)?.is_none() {
                debug!("Key chain has no key, generating");
                let key = SessionEncryptionKey::random();
                key_chain.store(key.to_base64()).map_err(|_e| {
                    tracing::error!("Failed to store key in keychain");
                    Unexpected::Os
                })?;
            }

            // Creating client.
            let api_env_config = params.api_env_config.unwrap_or_default();

            debug!("Creating Context");
            let mail_ctx = MailContext::new(
                session_path,
                user_path,
                core_cache_path,
                mail_cache_path,
                params.mail_cache_size,
                Arc::from(FFIKeyChain::from(key_chain)),
                api_env_config.into(),
                network_callback.map(|v| -> Box<dyn proton_core_common::NetworkStatusChanged> {
                    Box::new(FFINetworkStatusChanged::from(v))
                }),
            )
            .await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(MailSession {
                ctx: mail_ctx,
                _log_guard: log_guard,
            }))
        })
        .map_err(UserSessionError::from)
}

#[proton_uniffi_macros::export_result]
impl MailSession {
    /// Start new login flow.
    pub async fn new_login_flow(&self) -> Result<Arc<LoginFlow>, LoginError> {
        let ctx = self.ctx.clone();
        uniffi_async::<_, RealProtonMailError, _>(async move {
            ctx.new_login_flow()
                .map(|flow| LoginFlow::new(flow, ctx))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(LoginError::from)
    }

    /// Resume an existing login flow.
    pub async fn resume_login_flow(
        &self,
        user_id: String,
        session_id: String,
    ) -> Result<Arc<LoginFlow>, LoginError> {
        let ctx = self.ctx.clone();

        uniffi_async::<_, RealProtonMailError, _>(async move {
            let flow = ctx
                .resume_login_flow(user_id.into(), session_id.into())
                .await
                .map_err(RealProtonMailError::from)?;

            Ok(LoginFlow::new(flow, ctx))
        })
        .await
        .map_err(LoginError::from)
    }

    /// Create an user context from a stored session.
    pub fn user_context_from_session(
        &self,
        session: Arc<StoredSession>,
    ) -> Result<Arc<MailUserSession>, UserSessionError> {
        async_runtime()
            .block_on(async move {
                let ctx = self
                    .ctx
                    .user_context_from_session(session.session(), None)
                    .await?;

                Result::<_, RealProtonMailError>::Ok(MailUserSession::new(ctx))
            })
            .map_err(UserSessionError::from)
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
    pub async fn get_accounts(&self) -> Result<Vec<Arc<StoredAccount>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for account in ctx.get_accounts().await? {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(accounts)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Watch the accounts for changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_accounts(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<WatchedAccounts, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            let (initial, rx) = ctx.watch_accounts().await?;

            // TODO(ET-1431): Compute this on the core side.
            for account in initial {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_sync(accounts, rx, callback))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Watch the accounts for changes using an async callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_accounts_async(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<WatchedAccounts, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            let (initial, rx) = ctx.watch_accounts().await?;

            // TODO(ET-1431): Compute this on the core side.
            for account in initial {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_async(accounts, rx, callback))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get all API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_sessions(&self) -> Result<Vec<Arc<StoredSession>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for session in ctx.get_sessions().await? {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(sessions)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Watch all API sessions for changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_sessions(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<WatchedSessions, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_sessions().await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_sync(sessions, rx, callback))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Watch all API sessions for changes using an async callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_sessions_async(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<WatchedSessions, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_sessions().await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_async(sessions, rx, callback))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get all API sessions associated with a given account.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_account_sessions(
        &self,
        account: Arc<StoredAccount>,
    ) -> Result<Vec<Arc<StoredSession>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let account = account.account();

            let mut sessions = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for session in ctx.get_account_sessions(account.remote_id.clone()).await? {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(sessions)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Watch an account's API sessions for changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_account_sessions(
        &self,
        account: Arc<StoredAccount>,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<WatchedSessions, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_account_sessions(account.user_id().into()).await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                };
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_sync(sessions, rx, callback))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get a single account by its remote (user) ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account(
        &self,
        user_id: String,
    ) -> Result<Option<Arc<StoredAccount>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(account) = ctx.get_account(user_id.into()).await? else {
                return Ok(None);
            };

            // TODO(ET-1431): Compute this on the core side.
            let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? else {
                return Ok(None);
            };

            Result::<_, RealProtonMailError>::Ok(Some(StoredAccount::new(account, state)))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get a single API session by its associated account and session ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session(
        &self,
        session_id: String,
    ) -> Result<Option<Arc<StoredSession>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(session) = ctx.get_session(session_id.into()).await? else {
                return Ok(None);
            };

            // TODO(ET-1431): Compute this on the core side.
            let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? else {
                return Ok(None);
            };

            Result::<_, RealProtonMailError>::Ok(Some(StoredSession::new(session, state)))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get the login state of an account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account_state(
        &self,
        user_id: String,
    ) -> Result<Option<StoredAccountState>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let state = ctx
                .get_account_state(user_id.into())
                .await?
                .map(StoredAccountState::from);

            Result::<_, RealProtonMailError>::Ok(state)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get the login state of a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session_state(
        &self,
        session_id: String,
    ) -> Result<Option<StoredSessionState>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let state = ctx
                .get_session_state(session_id.into())
                .await?
                .map(StoredSessionState::from);

            Result::<_, RealProtonMailError>::Ok(state)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_primary_account(
        &self,
    ) -> Result<Option<Arc<StoredAccount>>, UserSessionError> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(account) = ctx.get_primary_account().await? else {
                return Ok(None);
            };

            let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? else {
                return Ok(None);
            };

            Result::<_, RealProtonMailError>::Ok(Some(StoredAccount::new(account, state)))
        })
        .await
        .map_err(UserSessionError::from)
    }
}

#[uniffi::export]
impl MailSession {
    /// Set the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the account is not found.
    pub async fn set_primary_account(&self, user_id: String) -> VoidSessionResult {
        let ctx = self.ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(ctx.set_primary_account(user_id).await?)
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }

    /// Logs out all sessions of an account without deleting the account's data.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn logout_account(&self, user_id: String) -> VoidSessionResult {
        let ctx = self.ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(ctx.logout_account(user_id).await?)
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }

    /// Removes an account and all associated sessions and data.
    ///
    /// # Errors
    ///
    /// Returns error if data can not be removed or the db operation failed.
    pub async fn delete_account(&self, user_id: String) -> VoidSessionResult {
        let ctx = self.ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(ctx.delete_account(user_id).await?)
        })
        .await
        .map_err(UserSessionError::from)
        .into()
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
    #[must_use]
    pub fn get_accounts_blocking(&self) -> MailSessionGetAccountsResult {
        async_runtime().block_on(self.get_accounts())
    }

    /// A blocking form of `get_account`.
    #[must_use]
    pub fn get_account_blocking(&self, user_id: String) -> MailSessionGetAccountResult {
        async_runtime().block_on(self.get_account(user_id))
    }

    /// A blocking form of `get_sessions`.
    #[must_use]
    pub fn get_sessions_blocking(
        &self,
        account: Arc<StoredAccount>,
    ) -> MailSessionGetAccountSessionsResult {
        async_runtime().block_on(self.get_account_sessions(account))
    }

    /// A blocking form of `get_session`.
    #[must_use]
    pub fn get_session_blocking(&self, session_id: String) -> MailSessionGetSessionResult {
        async_runtime().block_on(self.get_session(session_id))
    }

    /// A blocking form of `get_account_state`.
    #[must_use]
    pub fn get_account_state_blocking(&self, user_id: String) -> MailSessionGetAccountStateResult {
        async_runtime().block_on(self.get_account_state(user_id))
    }

    /// A blocking form of `get_session_state`.
    #[must_use]
    pub fn get_session_state_blocking(
        &self,
        session_id: String,
    ) -> MailSessionGetSessionStateResult {
        async_runtime().block_on(self.get_session_state(session_id))
    }

    /// A blocking form of `get_primary_account`.
    #[must_use]
    pub fn get_primary_account_blocking(&self) -> MailSessionGetPrimaryAccountResult {
        async_runtime().block_on(self.get_primary_account())
    }
}

impl MailSession {
    /// Get the mail context.
    #[must_use]
    pub fn ctx(&self) -> &MailContext {
        &self.ctx
    }

    /// Get the session database connection.
    #[must_use]
    pub fn session_stash(&self) -> &Stash {
        self.ctx.session_stash()
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
    fn new(accounts: Vec<Arc<StoredAccount>>, handle: Arc<WatchHandle>) -> Self {
        Self { accounts, handle }
    }

    fn new_sync(
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel(handle, callback))
    }

    fn new_async(
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel_async(handle, callback))
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
    fn new(sessions: Vec<Arc<StoredSession>>, handle: Arc<WatchHandle>) -> Self {
        Self { sessions, handle }
    }

    fn new_sync(
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel(handle, callback))
    }

    fn new_async(
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel_async(handle, callback))
    }
}
