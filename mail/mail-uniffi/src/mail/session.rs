use crate::core::datatypes::{ApiConfig, AppProtection, Id};
use crate::core::verification::{ChallengeNotifierWrap, DynChallengeNotifier};
use crate::core::{FFIKeyChain, StoredAccountState, StoredSession, StoredSessionState};
use crate::core::{OSKeyChain, StoredAccount};
use crate::errors::{LoginError, PinAuthError, PinSetError, UserSessionError, VoidSessionResult};
use crate::mail::logging::init_log;
use crate::mail::state::MailUserContextMap;
use crate::mail::{LoginFlow, MailUserSession};
use crate::{AsyncLiveQueryCallback, watch_channel_async};
use crate::{
    LiveQueryCallback, WatchHandle, async_runtime, async_runtime_slim, spawn_async, uniffi_async,
    watch_channel,
};
use futures::TryFutureExt;
use itertools::Itertools;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::models::AppSettings;
use proton_core_common::os::KeyChainExt;
use proton_core_common::pin_code::PinCode;
use proton_core_common::{CoreAccountState, CoreSessionState};
use proton_mail_common::actions::draft::Send;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_mail_common::models::DraftMetadata;
use proton_mail_common::{MailContext, MailUserContext};
use stash::stash::{Stash, WatcherHandle};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, spawn_blocking};
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
    mail_ctx: Arc<MailContext>,
    user_ctx: Arc<MailUserContextMap>,
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

    // Creating client.
    let api_env_config = params.api_env_config.unwrap_or_default();
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
        api_env_config.into(),
        hv_notifier,
        Some(log_path),
    )
    .await?;

    Ok(Arc::new(MailSession {
        mail_ctx,
        user_ctx: MailUserContextMap::new(),
        _log_guard: log_guard,
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

    /// Create an user context from a stored session.
    pub fn user_context_from_session(
        &self,
        session: Arc<StoredSession>,
    ) -> Result<Arc<MailUserSession>, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        async_runtime()
            .block_on(async move {
                ctx.user_context_from_session(session.session(), None)
                    .map_ok(|ctx| self.user_ctx.insert(ctx))
                    .map_ok(MailUserSession::new)
                    .map_err(RealProtonMailError::from)
                    .await
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

    /// Functionality to execute pending actions for all logged in accounts in controlled manner.
    ///
    /// This method is ment to be executed when putting application to sleep or running it in the background.
    /// It stops automatic execution of the queues and sequentially execute actions in following priority:
    /// * Send actions for primary account,
    /// * Send actions for secondary accounts,
    /// * Other actions for primary account,
    /// * Other actions for secondary accounts,
    ///
    /// It will stop when aborded or when finished whatever comes first.
    /// On exit the callback will be triggered to notify caller that it finished.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn start_background_execution(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<Arc<BackgroundExecutionHandle>, UserSessionError> {
        let ctx = self.mail_ctx.clone();
        let (sender, abort) = mpsc::channel(1);

        let handle = spawn_async(ctx.clone(), async move {
            let all_user_ctxs = get_all_logged_in_mail_user_contexts(&ctx)
                .await
                .inspect_err(|e| {
                    tracing::error!("Failed to get logged in users, details: `{e:?}`");
                })?;

            if all_user_ctxs.is_empty() {
                tracing::warn!("There are no logged in users, skipping background execution");
                let callback = move || callback.on_update();
                let _ = spawn_blocking(move || callback()).await.inspect_err(|e| {
                    tracing::error!(
                        "Could not call callback in background execution, details: `{e}`"
                    );
                });
                return Ok(());
            }

            let mut execution_ctx = BackgroundExecutionContext {
                abort,
                _musc: all_user_ctxs,
            };

            tracing::debug!("Background execution is in progress... awaiting for abort");
            execution_ctx.abort.recv().await;
            execution_ctx.stop();

            Result::<_, RealProtonMailError>::Ok(())
        });

        Ok(Arc::new(BackgroundExecutionHandle {
            sender,
            handle: handle.abort_handle(),
            ctx: Arc::downgrade(&self.mail_ctx),
        }))
    }

    /// Check if any message for all logged in accounts is still pending to send
    ///
    pub async fn all_messages_were_sent(&self) -> Result<bool, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let all_user_ctxs = get_all_logged_in_mail_user_contexts(&ctx).await?;
            let mut all_messages_were_sent = true;

            for user_ctx in &all_user_ctxs {
                let send_task_count_eq_zero = user_ctx
                    .action_queue()
                    .typed_actions_count::<Send>()
                    .await?
                    == 0;

                all_messages_were_sent &= send_task_count_eq_zero;
            }

            Result::<_, RealProtonMailError>::Ok(all_messages_were_sent)
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
            let session = ctx.get_account_sessions(user_id.into()).await?.pop();

            let msg_ids = match session {
                Some(session)
                    if matches!(
                        ctx.get_session_state(session.remote_id.clone()).await?,
                        Some(CoreSessionState::Authenticated)
                    ) =>
                {
                    let user_ctx = ctx.user_context_from_session(&session, None).await?;
                    let tether = user_ctx.user_stash().connection();

                    DraftMetadata::messages_with_pending_send(&tether)
                        .await?
                        .into_iter()
                        .map_into()
                        .collect()
                }
                _ => vec![],
            };

            Result::<_, RealProtonMailError>::Ok(msg_ids)
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn app_protection(&self) -> Result<AppProtection, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let tether = ctx.core_context().account_stash().connection();
            let app_settings = AppSettings::get_or_default(&tether).await;

            Result::<_, RealProtonMailError>::Ok(app_settings.protection.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn set_pin_code(&self, pin: Vec<u8>) -> Result<(), PinSetError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::create_pin(&ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinSetError::from)
    }

    pub async fn verify_pin_code(&self, pin: Vec<u8>) -> Result<(), PinAuthError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::validate_pin(&ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinAuthError::from)
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

/// Handle for background activites execution.
///
/// It is meant to be hold by a caller of `start_background_execution` method.
/// When dropped it will cease the execution.
///
#[derive(uniffi::Object)]
pub struct BackgroundExecutionHandle {
    sender: mpsc::Sender<()>,
    handle: AbortHandle,
    ctx: Weak<MailContext>,
}

#[uniffi_export]
impl BackgroundExecutionHandle {
    /// Abort background execution.
    ///
    /// Allows holder of the `BackgroundExecutionHandle` to finish execution prematurely.
    ///
    pub async fn abort(&self) {
        if !self.sender.is_closed() && !self.handle.is_finished() {
            if let Err(e) = self.sender.send(()).await {
                tracing::error!(
                    "Critical: Could not notify task to abort, force it to finish, details: `{e}`"
                );
                self.handle.abort();
            }
        }
    }
}

impl Drop for BackgroundExecutionHandle {
    fn drop(&mut self) {
        let sender = self.sender.clone();
        let handle = self.handle.clone();
        if let Some(ctx) = self.ctx.upgrade() {
            spawn_async(ctx, async move {
                if !sender.is_closed() && !handle.is_finished() {
                    if let Err(e) = sender.send(()).await {
                        tracing::error!(
                            "Critical: Could not notify task to abort on drop, force it to finish, details: `{e}`"
                        );
                        handle.abort();
                    }
                }
            });
        } else {
            tracing::warn!(
                "MailContext already dropped, background execution handle should not live that long"
            );
        }
    }
}

/// Internal representation of Execution.
///
/// It purpuose is to group all the operations which needs to happen,
/// when execution is aborted.
///
struct BackgroundExecutionContext {
    abort: mpsc::Receiver<()>,
    _musc: Vec<Arc<MailUserContext>>,
}

impl BackgroundExecutionContext {
    #[allow(clippy::unused_self)]
    pub fn stop(self) {
        tracing::debug!("Stoping execution of background activites");
    }
}

async fn get_all_logged_in_mail_user_contexts(
    ctx: &Arc<MailContext>,
) -> Result<Vec<Arc<MailUserContext>>, RealProtonMailError> {
    let Some(primary_account) = ctx.get_primary_account().await? else {
        tracing::warn!("Missing primary account, skipping background execution");
        return Ok(vec![]);
    };
    let Some(session) = ctx
        .get_account_sessions(primary_account.remote_id.clone())
        .await?
        .pop()
    else {
        tracing::warn!("No active session for primary account, skipping background execution");
        return Ok(vec![]);
    };
    let primary_user_ctx = ctx.user_context_from_session(&session, None).await?;

    let other_sessions_iter = ctx
        .get_sessions()
        .await?
        .into_iter()
        .filter(|session| session.account_id != primary_account.remote_id)
        .unique_by(|session| session.account_id.clone());

    // There might be a case when a User logs out the primary account before putting app in the background
    let mut all_user_ctxs = if let Some(CoreAccountState::LoggedIn(_)) = ctx
        .get_account_state(primary_user_ctx.user_id().clone())
        .await?
    {
        vec![primary_user_ctx.clone()]
    } else {
        tracing::warn!("Primary account is not LoggedIn");
        vec![]
    };

    for session in other_sessions_iter {
        // Make sure we deal only with Authenticated Sessions
        let Some(CoreSessionState::Authenticated) =
            ctx.get_session_state(session.remote_id.clone()).await?
        else {
            tracing::warn!(
                "Found unauthenticated session for secondary account, this may suggest problem with loggin flow not correctly resumed"
            );
            continue;
        };

        let user_ctx = ctx.user_context_from_session(&session, None).await?;
        all_user_ctxs.push(user_ctx);
    }

    Ok(all_user_ctxs)
}
