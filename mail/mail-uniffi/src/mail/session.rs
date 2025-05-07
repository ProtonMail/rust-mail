use crate::core::datatypes::{ApiConfig, AppProtection, AppSettings, AppSettingsDiff, Id};
use crate::core::verification::{ChallengeNotifierWrap, DynChallengeNotifier};
use crate::core::{FFIKeyChain, StoredAccountState, StoredSession, StoredSessionState};
use crate::core::{OSKeyChain, StoredAccount};
use crate::errors::{LoginError, PinAuthError, PinSetError, UserSessionError, VoidSessionResult};
use crate::mail::logging::init_log;
use crate::mail::state::MailUserContextMap;
use crate::mail::{LoginFlow, MailUserSession};
use crate::{AsyncLiveQueryCallback, watch_channel_async};
use crate::{
    LiveQueryCallback, WatchHandle, async_runtime, async_runtime_slim, uniffi_async, watch_channel,
};
use futures::TryFutureExt;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::models::{AppSettings as RealAppSettings, PinProtection};
use proton_core_common::os::KeyChainExt;
use proton_core_common::pin_code::PinCode;
use proton_core_common::utils::MapVec;
use proton_mail_common::MailContext;
use proton_mail_common::context::{EventPollMode, ShouldInitializeMailUserContext};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::unexpected::Unexpected;
use stash::stash::{Stash, WatcherHandle};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error};
use tracing_appender::non_blocking::WorkerGuard;

/// Mail context is the entry point for the application. It contains important state such as
/// database connection pools and the async runtime for rust.
///
/// # Lifetime
/// This object needs to be kept alive for the entire duration of the application.
///
#[derive(uniffi::Object)]
pub struct MailSession {
    mail_ctx: Arc<MailContext>,
    user_ctx: Arc<MailUserContextMap>,
    /// This is an Option because it compiles to `None` for iOS.
    _log_guard: Option<WorkerGuard>,
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
///
/// # Panics
///
/// Panics if the API URL is invalid. In this situation we cannot proceed.
///
/// TODO: An error type needs to be added for this later.
///
#[must_use]
#[uniffi_export]
pub fn create_mail_session(
    params: MailSessionParams,
    key_chain: Box<dyn OSKeyChain>,
    hv_notifier: Option<DynChallengeNotifier>,
) -> Result<Arc<MailSession>, UserSessionError> {
    async_runtime()
        .block_on(
            async move { create_mail_session_inner(params, None, key_chain, hv_notifier).await },
        )
        .map_err(UserSessionError::from)
}

// NOTE: Callbacks can not be stored in record types, which is why they are still in the
// constructor.
/// Create a new mail session with a slim async runtime.
///
/// Comparing to [`create_mail_session`] it uses less async task workers and blocking threads, and
/// operates on lower number of connections, lowering memory consumption in more resource
/// constrained devices and applications.
///
/// # Warning
///
/// This function is designed for extension. Do not use it in the main application without thorough testing!
///
/// # Parameters
///
/// * `params`: See [`MailSessionParams`] for parameter details.
/// * `key_chain`: Keychain implementation.
///
/// # Panics
///
/// Panics if the API URL is invalid. In this situation we cannot proceed.
///
/// TODO: An error type needs to be added for this later.
///
#[must_use]
#[uniffi_export]
pub fn create_mail_ios_extension_session(
    params: MailSessionParams,
    key_chain: Box<dyn OSKeyChain>,
) -> Result<Arc<MailSession>, UserSessionError> {
    // This number is arbitrary
    async_runtime_slim()
        .block_on(async move { create_mail_session_inner(params, Some(4), key_chain, None).await })
        .map_err(UserSessionError::from)
}

// NOTE: Callbacks can not be stored in record types, which is why they are still in the
// constructor.
/// Create a new mail session.
///
/// # Parameters
///
/// * `params`: See [`MailSessionParams`] for parameter details.
/// * `connection_pool_size`: Maximum number of connections for account DB. If `None`,
///   then default value is used
/// * `key_chain`: Keychain implementation.
///
/// # Panics
///
/// Panics if the API URL is invalid. In this situation we cannot proceed.
///
/// TODO: An error type needs to be added for this later.
///
async fn create_mail_session_inner(
    params: MailSessionParams,
    connection_pool_size: Option<u32>,
    key_chain: Box<dyn OSKeyChain>,
    hv_notifier: Option<DynChallengeNotifier>,
) -> Result<Arc<MailSession>, RealProtonMailError> {
    let mut log_path = PathBuf::from(params.log_dir);
    std::fs::create_dir_all(&log_path)?;
    log_path.push("proton-mail-uniffi.log");

    let maybe_log_guard = init_log(&log_path, params.log_debug)?;

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
    let key_chain = FFIKeyChain(key_chain);
    if key_chain
        .load::<SessionEncryptionKey>()
        .map_err(|_| Unexpected::Os)?
        .is_none()
    {
        debug!("Key chain has no key, generating");
        let key = SessionEncryptionKey::random();
        key_chain.store(key).map_err(|_e| {
            tracing::error!("Failed to store key in keychain");
            Unexpected::Os
        })?;
    }

    let api_env_config = params
        .api_env_config
        .unwrap_or_default()
        .try_into()
        .inspect_err(|e| error!("{e:?}"))
        .map_err(|_| Unexpected::Config)?;

    let hv_notifier = hv_notifier.map(ChallengeNotifierWrap::wrap);

    debug!("Creating Context");
    let mail_ctx = MailContext::new(
        session_path,
        user_path,
        core_cache_path,
        mail_cache_path,
        params.mail_cache_size,
        connection_pool_size,
        Arc::new(key_chain),
        api_env_config,
        hv_notifier,
        Some(log_path),
        EventPollMode::Automatic(Duration::from_secs(30)),
    )
    .await?;

    Ok(Arc::new(MailSession {
        mail_ctx,
        user_ctx: MailUserContextMap::new(),
        _log_guard: maybe_log_guard,
    }))
}

#[uniffi_export]
impl MailSession {
    /// Start new login flow.
    pub async fn new_login_flow(&self) -> Result<Arc<LoginFlow>, LoginError> {
        let mail_ctx = self.mail_ctx.clone();
        let user_ctx = self.user_ctx.clone();

        uniffi_async::<_, RealProtonMailError, _>(async move {
            mail_ctx
                .new_login_flow()
                .await
                .map(|flow| LoginFlow::new(flow, mail_ctx, user_ctx))
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
        let mail_ctx = self.mail_ctx.clone();
        let user_ctx = self.user_ctx.clone();

        uniffi_async::<_, RealProtonMailError, _>(async move {
            Arc::clone(&mail_ctx)
                .resume_login_flow(user_id.into(), session_id.into())
                .map_ok(|flow| LoginFlow::new(flow, mail_ctx, user_ctx))
                .map_err(RealProtonMailError::from)
                .await
        })
        .await
        .map_err(LoginError::from)
    }

    /// Get initialized user context from stored session.
    /// If context exists but it is not initialized yet, it returns `None`.
    ///
    /// This method **does NOT** initialize context itself.
    pub async fn initialized_user_context_from_session(
        self: Arc<Self>,
        session: Arc<StoredSession>,
    ) -> Result<Option<Arc<MailUserSession>>, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        let user_ctx = self.user_ctx.clone();
        let weak_user_ctx = Arc::downgrade(&user_ctx);
        let user_ctx = uniffi_async(async move {
            ctx.initialized_user_context_from_session(
                session.session(),
                None,
                move |_session_id, user_id| async move {
                    tracing::warn!("Session ended. Removing from the map");
                    if let Some(ctx) = weak_user_ctx.upgrade() {
                        ctx.remove(&user_id);
                    }
                },
            )
            .map_err(RealProtonMailError::from)
            .await
            .map(|ctx| ctx.map(|ctx| user_ctx.insert(ctx)))
        })
        .map_ok(|ctx| ctx.map(MailUserSession::new))
        .await?;

        Ok(user_ctx)
    }

    /// Create an user context from a stored session.
    pub async fn user_context_from_session(
        &self,
        session: Arc<StoredSession>,
    ) -> Result<Arc<MailUserSession>, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        let user_ctx = self.user_ctx.clone();
        let user_ctx = uniffi_async(async move {
            let weak_user_ctx = Arc::downgrade(&user_ctx);
            ctx.user_context_from_session(
                session.session(),
                None,
                ShouldInitializeMailUserContext::Yes,
                move |_session_id, user_id| async move {
                    tracing::warn!("Session ended. Removing from the map");
                    if let Some(ctx) = weak_user_ctx.upgrade() {
                        ctx.remove(&user_id);
                    }
                },
            )
            .map_err(RealProtonMailError::from)
            .await
            .map(|ctx| user_ctx.insert(ctx))
        })
        .map_ok(MailUserSession::new)
        .await?;

        Ok(user_ctx)
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for account in ctx.get_accounts().await? {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                }
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            let (initial, rx) = ctx.watch_accounts().await?;

            // TODO(ET-1431): Compute this on the core side.
            for account in initial {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                }
            }

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_sync(
                ctx, accounts, rx, callback,
            ))
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut accounts = Vec::new();

            let (initial, rx) = ctx.watch_accounts().await?;

            // TODO(ET-1431): Compute this on the core side.
            for account in initial {
                if let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? {
                    accounts.push(StoredAccount::new(account, state));
                }
            }

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_async(
                ctx, accounts, rx, callback,
            ))
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for session in ctx.get_sessions().await? {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                }
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_sessions().await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                }
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_sync(
                ctx, sessions, rx, callback,
            ))
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_sessions().await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                }
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_async(
                ctx, sessions, rx, callback,
            ))
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let account = account.account();

            let mut sessions = Vec::new();

            // TODO(ET-1431): Compute this on the core side.
            for session in ctx.get_account_sessions(account.remote_id.clone()).await? {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                }
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
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let mut sessions = Vec::new();

            let (initial, rx) = ctx.watch_account_sessions(account.user_id().into()).await?;

            // TODO(ET-1431): Compute this on the core side.
            for session in initial {
                if let Some(state) = ctx.get_session_state(session.remote_id.clone()).await? {
                    sessions.push(StoredSession::new(session, state));
                }
            }

            Result::<_, RealProtonMailError>::Ok(WatchedSessions::new_sync(
                ctx, sessions, rx, callback,
            ))
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
        let ctx = self.mail_ctx.clone();

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
        let ctx = self.mail_ctx.clone();

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
        let ctx = self.mail_ctx.clone();

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
        let ctx = self.mail_ctx.clone();

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
        let ctx = self.mail_ctx.clone();

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

    /// Check if any message for all logged in accounts is still pending to send
    ///
    pub async fn all_messages_were_sent(&self) -> Result<bool, UserSessionError> {
        let ctx = self.mail_ctx.clone();
        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(ctx.has_users_with_unsent_messages().await?)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get all unsent message ids for given user id
    ///
    pub async fn get_unsent_messages_ids_in_queue(
        &self,
        user_id: String,
    ) -> Result<Vec<Id>, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(
                ctx.get_unsent_messages_ids_for_user(user_id.into())
                    .await?
                    .into_iter()
                    .map_vec(),
            )
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// What aditional protection of the App is configured.
    ///
    pub async fn app_protection(&self) -> Result<AppProtection, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let tether = ctx.core_context().account_stash().connection();
            let app_settings = RealAppSettings::get_or_default(&tether).await;

            Result::<_, RealProtonMailError>::Ok(app_settings.protection.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Create a PIN App protection.
    ///
    /// The same PIN will be required for authentication of the user
    /// or when user want to change the way of authentication in the App.
    ///
    pub async fn set_pin_code(&self, pin: Vec<u32>) -> Result<(), PinSetError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::create_pin(ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinSetError::from)
    }

    /// Authenticate stored PIN
    ///
    pub async fn verify_pin_code(&self, pin: Vec<u32>) -> Result<(), PinAuthError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::validate_pin(ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinAuthError::from)
    }

    /// Delete stored PIN
    ///
    /// This method also carries verification of the PIN to remove, that is why
    /// it returns PinAuthError type. If verification is unsuccessful it won't
    /// remove the PIN and return proper Error Reason.
    ///
    pub async fn delete_pin_code(&self, pin: Vec<u32>) -> Result<(), PinAuthError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::delete_pin(ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinAuthError::from)
    }

    /// Return remaining attempts at verifing PIN code.
    ///
    /// Method will return None when PIN protection is not set.
    /// Method will return Some(value) when PIN protection is in use.
    ///
    pub async fn remaining_pin_attempts(&self) -> Result<Option<u32>, UserSessionError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            let tether = ctx.account_stash().connection();
            let pin_metadata = PinProtection::get(&tether).await?;
            let remaining_attempts = match pin_metadata {
                Some(pin_metadata) => Some(pin_metadata.remaining_attempts()),
                None => None,
            };

            Result::<_, RealProtonMailError>::Ok(remaining_attempts)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Set App Protection to `Biometrics`
    ///
    /// This function will have no effect if the current `AppProtection` is something
    /// different than None.
    ///
    pub async fn set_biometrics_app_protection(&self) -> Result<(), UserSessionError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            let mut tether = ctx.account_stash().connection();
            let mut app_settings = RealAppSettings::get_or_default(&tether).await;
            app_settings.set_biometrics();

            tether.tx(async |tx| app_settings.save(tx).await).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Set App Protection to `None`
    ///
    /// This function will have no effect if the current `AppProtection` is something
    /// different than Biometrics.
    ///
    pub async fn unset_biometrics_app_protection(&self) -> Result<(), UserSessionError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            let mut tether = ctx.account_stash().connection();
            let mut app_settings = RealAppSettings::get_or_default(&tether).await;
            app_settings.unset_biometrics();

            tether.tx(async |tx| app_settings.save(tx).await).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get current app settings
    ///
    pub async fn get_app_settings(&self) -> Result<AppSettings, UserSessionError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            let tether = ctx.account_stash().connection();
            let app_settings = RealAppSettings::get_or_default(&tether).await;

            Result::<_, RealProtonMailError>::Ok(app_settings.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Change the settings of the application.
    ///
    pub async fn change_app_settings(
        &self,
        settings: AppSettingsDiff,
    ) -> Result<(), UserSessionError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            let mut tether = ctx.account_stash().connection();
            let real_app_settings = RealAppSettings::get_or_default(&tether).await;
            let mut real_app_settings = settings.merge_with_current(real_app_settings);

            tether
                .tx(async |tx| real_app_settings.save(tx).await)
                .await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
    }
}

#[uniffi_export]
impl MailSession {
    /// Set the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the account is not found.
    #[returns(VoidSessionResult)]
    pub async fn set_primary_account(&self, user_id: String) -> Result<(), UserSessionError> {
        let ctx = self.mail_ctx.clone();
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
    #[returns(VoidSessionResult)]
    pub async fn logout_account(&self, user_id: String) -> Result<(), UserSessionError> {
        let user_ctx = self.user_ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            user_ctx
                .remove(&user_id)
                .ok_or(Unexpected::Internal)?
                .logout()
                .map_err(RealProtonMailError::from)
                .await
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
    #[returns(VoidSessionResult)]
    pub async fn delete_account(&self, user_id: String) -> Result<(), UserSessionError> {
        let mail_ctx = self.mail_ctx.clone();
        let user_ctx = self.user_ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            if user_ctx.remove(&user_id).is_none() {
                debug!("Deleting account without any active context");
            }

            mail_ctx
                .delete_account(user_id)
                .map_err(RealProtonMailError::from)
                .await
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }
}

#[uniffi_export]
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

    /// Pause all background work
    ///
    /// This should be called once the application enters the background.
    pub fn pause_work(&self) {
        self.mail_ctx.core_context().task_service().pause_main();
    }

    /// Pause all background work and wait for all non-pausable futures to complete.
    ///
    /// This should be called once the application enters the background.
    pub fn pause_work_and_wait(&self) {
        async_runtime().block_on(async {
            if let Err(e) = self
                .mail_ctx
                .core_context()
                .task_service()
                .pause_main_and_wait(Duration::from_millis(100))
                .await
            {
                error!("Failed to await paused work: {e:?}");
            }
        });
    }

    /// Resume all background work
    ///
    /// This should be called once the application enters the foreground.
    pub fn resume_work(&self) {
        self.mail_ctx.core_context().task_service().resume_main();
    }
}

impl MailSession {
    /// Get the mail context.
    #[must_use]
    pub fn ctx(&self) -> &MailContext {
        &self.mail_ctx
    }

    /// Get the mail context wrapped in [`Arc`]
    #[must_use]
    pub fn ctx_arc(&self) -> Arc<MailContext> {
        Arc::clone(&self.mail_ctx)
    }

    /// Get the session database connection.
    #[must_use]
    pub fn session_stash(&self) -> &Stash {
        self.mail_ctx.session_stash()
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
        ctx: impl AsRef<MailContext>,
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel(ctx, handle, callback))
    }

    fn new_async(
        ctx: impl AsRef<MailContext>,
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel_async(ctx, handle, callback))
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
        ctx: impl AsRef<MailContext>,
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel(ctx, handle, callback))
    }

    fn new_async(
        ctx: impl AsRef<MailContext>,
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel_async(ctx, handle, callback))
    }
}
