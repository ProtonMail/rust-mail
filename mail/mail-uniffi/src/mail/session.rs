use crate::core::datatypes::{ApiConfig, AppDetails, AppProtection, AppSettings, AppSettingsDiff};
use crate::core::device::{DeviceInfoProviderWrap, DynDeviceInfoProvider};
use crate::core::verification::{ChallengeNotifierWrap, DynChallengeNotifier};
use crate::core::{FFIKeyChain, StoredAccountState, StoredSession, StoredSessionState};
use crate::core::{OSKeyChain, StoredAccount};
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{
    OtherErrorReason, PinAuthError, PinSetError, ProtonError, SessionReason, UserSessionError,
    VoidSessionResult,
};
use crate::mail::MailUserSession;
use crate::mail::logging::init_log;
use crate::mail::state::MailUserContextMap;
use crate::version::rust_sdk_version;
use crate::{AsyncLiveQueryCallback, watch_channel_async};
use crate::{
    LiveQueryCallback, WatchHandle, async_runtime, async_runtime_slim, uniffi_async, watch_channel,
};
use proton_core_common::services::SessionObserverService;

use chrono::Local;
use futures::TryFutureExt;
use proton_account_uniffi::login::LoginFlow;
use proton_account_uniffi::signup::SignupFlow;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::{AppSettings as RealAppSettings, PinProtection};
use proton_core_common::os::KeyChainExt;
use proton_core_common::pin_code::PinCode;
use proton_core_common::{CoreContextError, OnSessionDeletedResponse, Origin as RealOrigin};
use proton_issue_reporter_service_uniffi::{IssueReporter, IssueReporterWrapper};
use proton_log_service::LogService;
use proton_mail_common::context::ShouldInitializeMailUserContext;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_mail_common::errors::{
    ContextErrorReason, MailErrorReason, ProtonMailError as RealProtonMailError,
};
use proton_mail_common::{MailContext, MailContextError};
use proton_network_monitor_service::OsNetworkStatus as RealOsNetworkStatus;
use stash::orm::Model;
use stash::stash::{Stash, WatcherHandle};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Origin {
    App,
    IosShareExt,
}

impl Origin {
    pub fn guard(self, expected: Self) -> Result<(), UserSessionError> {
        if self != expected {
            return Err(UserSessionError::Reason(
                SessionReason::MethodCalledInWrongOrigin {
                    expected,
                    actual: self,
                },
            ));
        }
        Ok(())
    }
}

impl From<Origin> for RealOrigin {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::App => Self::App,
            Origin::IosShareExt => Self::ShareExt,
        }
    }
}

/// Mail session is the entry point for the application. It contains important state such as
/// database connection pools and the async runtime for rust.
///
/// # Lifetime
/// This object needs to be kept alive for the entire duration of the application.
///
#[derive(uniffi::Object)]
#[allow(dead_code)]
pub struct MailSession {
    mail_ctx: Arc<MailContext>,
    user_ctx: Arc<MailUserContextMap>,
    params: MailSessionParams,
}

/// Configuration parameters for the [`MailSession`]
#[derive(uniffi::Record)]
pub struct MailSessionParams {
    pub origin: Origin,
    pub session_dir: String,
    pub user_dir: String,
    pub mail_cache_dir: String,
    pub mail_cache_size: u64,
    pub log_dir: String,
    pub log_debug: bool,
    pub api_env_config: Option<ApiConfig>,
    pub app_details: AppDetails,
    pub event_poll_duration_seconds: Option<u64>,
}

// NOTE: Callbacks can not be stored in record types, which is why they are still in the
// constructor.
/// Create a new mail session.
///
#[must_use]
#[uniffi_export]
pub fn create_mail_session(
    params: MailSessionParams,
    key_chain: Box<dyn OSKeyChain>,
    hv_notifier: Option<DynChallengeNotifier>,
    device_info_provider: Option<DynDeviceInfoProvider>,
    issue_reporter: Arc<dyn IssueReporter>,
) -> Result<Arc<MailSession>, UserSessionError> {
    let runtime = match params.origin {
        Origin::App => async_runtime,
        Origin::IosShareExt => async_runtime_slim,
    };

    runtime()
        .block_on(async move {
            create_mail_session_inner(
                params,
                key_chain,
                hv_notifier,
                device_info_provider,
                issue_reporter,
            )
            .await
        })
        .map_err(UserSessionError::from)
}

// NOTE: Callbacks can not be stored in record types, which is why they are still in the
// constructor.
async fn create_mail_session_inner(
    params: MailSessionParams,
    key_chain: Box<dyn OSKeyChain>,
    hv_notifier: Option<DynChallengeNotifier>,
    device_info_provider: Option<DynDeviceInfoProvider>,
    issue_reporter: Arc<dyn IssueReporter>,
) -> Result<Arc<MailSession>, RealProtonMailError> {
    let log_path = PathBuf::from(&params.log_dir);
    std::fs::create_dir_all(&log_path)?;

    let log_service = LogService::new(
        proton_log_service::Config::builder()
            .name("proton-mail-uniffi".into())
            .header(|| {
                format!(
                    "\n ---- Proton Mail Uniffi ({}) ---- Started at {}\n",
                    rust_sdk_version(),
                    Local::now()
                )
            })
            .directory(log_path)
            .build(),
    );

    init_log(&log_service, params.log_debug)?;

    let session_path = PathBuf::from(&params.session_dir);
    let user_path = PathBuf::from(&params.user_dir);
    let cache_path = PathBuf::from(&params.mail_cache_dir);
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
        .clone()
        .unwrap_or_default()
        .into_real_api_config(params.app_details.clone())
        .inspect_err(|e| error!("Failed to get api_env_config {e:?}"))
        .map_err(|_| Unexpected::Config)?;

    let hv_notifier = hv_notifier.map(ChallengeNotifierWrap::wrap);
    let device_info_provider = device_info_provider.map(DeviceInfoProviderWrap::wrap);

    debug!(origin = ?params.origin, "Creating Context");

    let poll = match params.origin {
        Origin::App => EventPollMode::Automatic(Duration::from_secs(
            params.event_poll_duration_seconds.unwrap_or(30),
        )),
        Origin::IosShareExt => EventPollMode::Manual,
    };

    let mail_ctx = MailContext::new(
        params.origin.into(),
        async_runtime().handle().clone(),
        session_path,
        user_path,
        core_cache_path,
        mail_cache_path,
        params.mail_cache_size,
        Arc::new(key_chain),
        api_env_config,
        hv_notifier,
        device_info_provider,
        log_service,
        poll,
        proton_network_monitor_service::Config::default(),
        IssueReporterWrapper::new(issue_reporter),
    )
    .await?;

    let user_ctx_map = MailUserContextMap::new();
    let weak_user_ctx_map = Arc::downgrade(&user_ctx_map);

    let core_ctx = mail_ctx.core_context();
    if let Some(session_service) = core_ctx.get_service_opt::<SessionObserverService>() {
        let event_service = core_ctx.event_service();
        session_service.on_session_deleted(event_service, move |_session_id, user_id| {
            let weak_user_ctx_map = weak_user_ctx_map.clone();
            async move {
                tracing::warn!("Session ended. Removing from the map");
                if let Some(ctx) = weak_user_ctx_map.upgrade() {
                    ctx.remove(&user_id);
                    OnSessionDeletedResponse::Continue
                } else {
                    OnSessionDeletedResponse::Terminate
                }
            }
        });
    }

    Ok(Arc::new(MailSession {
        mail_ctx,
        user_ctx: user_ctx_map,
        params,
    }))
}

#[uniffi_export]
impl MailSession {
    /// Start new login flow.
    pub async fn new_login_flow(&self) -> Result<Arc<LoginFlow>, ProtonError> {
        let mail_ctx = self.mail_ctx.clone();

        uniffi_async::<_, CoreContextError, _>(async move {
            mail_ctx
                .new_login_flow()
                .await
                .map(|flow| LoginFlow::new(flow))
        })
        .await
        .map_err(|err| ProtonError::OtherReason(OtherErrorReason::Other(err.to_string())))
    }

    /// Resume an existing login flow.
    pub async fn resume_login_flow(
        &self,
        user_id: String,
        session_id: String,
    ) -> Result<Arc<LoginFlow>, ProtonError> {
        let mail_ctx = self.mail_ctx.clone();

        uniffi_async::<_, RealProtonMailError, _>(async move {
            Arc::clone(&mail_ctx)
                .resume_login_flow(user_id.into(), session_id.into())
                .map_ok(|flow| LoginFlow::new(flow))
                .map_err(RealProtonMailError::from)
                .await
        })
        .await
        .map_err(ProtonError::from)
    }

    /// Start new signup flow.
    pub async fn new_signup_flow(&self) -> Result<Arc<SignupFlow>, ProtonError> {
        let mail_ctx = self.mail_ctx.clone();

        uniffi_async::<_, CoreContextError, _>(async move {
            mail_ctx
                .new_signup_flow()
                .await
                .map(|flow| SignupFlow::new(flow))
        })
        .await
        .map_err(|err| ProtonError::OtherReason(OtherErrorReason::Other(err.to_string())))
    }

    /// Get initialized user session from stored session.
    /// If session exists but it is not initialized yet, it returns `None`.
    ///
    /// This method **does NOT** initialize session itself.
    pub async fn initialized_user_session_from_stored_session(
        self: Arc<Self>,
        session: Arc<StoredSession>,
    ) -> Result<Option<Arc<MailUserSession>>, UserSessionError> {
        self.params.origin.guard(Origin::App)?;
        let ctx = self.mail_ctx.clone();

        let user_ctx = self.user_ctx.clone();
        let user_ctx = uniffi_async(async move {
            ctx.initialized_user_context_from_session(session.session())
                .map_err(RealProtonMailError::from)
                .await
                .map(|ctx| ctx.map(|ctx| user_ctx.insert(ctx)))
        })
        .map_ok(|ctx| ctx.map(MailUserSession::new))
        .await?;

        Ok(user_ctx)
    }

    /// Create an user session from a stored session.
    pub async fn user_session_from_stored_session(
        &self,
        session: Arc<StoredSession>,
    ) -> Result<Arc<MailUserSession>, UserSessionError> {
        self.params.origin.guard(Origin::App)?;
        let ctx = self.mail_ctx.clone();

        let user_ctx = self.user_ctx.clone();
        let user_ctx = uniffi_async(async move {
            ctx.user_context_from_session(session.session(), ShouldInitializeMailUserContext::Yes)
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

            accounts.sort_by_cached_key(|a| a.details().name.to_lowercase());

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

            accounts.sort_by_cached_key(|a| a.details().name.to_lowercase());

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_sync(
                &*ctx, accounts, rx, callback,
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

            accounts.sort_by_cached_key(|a| a.details().name.to_lowercase());

            Result::<_, RealProtonMailError>::Ok(WatchedAccounts::new_async(
                &*ctx, accounts, rx, callback,
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
                &*ctx, sessions, rx, callback,
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
                &*ctx, sessions, rx, callback,
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
                &*ctx, sessions, rx, callback,
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

    /// Sing out from all accounts.
    ///
    /// This method is going to remove all user data & account data
    /// associated with the mail application.
    ///
    /// This method is meant to be used when someone decides to sign out on
    /// authentication screen such as PIN or Biometrics verification.
    ///
    /// This method will recover an empty state of the mail application ready to
    /// log user back in.
    ///
    pub async fn sign_out_all(&self) -> Result<(), UserSessionError> {
        let Some(user_context) = self.user_ctx.first() else {
            tracing::debug!("No user context found, skipping sign out all");
            return Ok(());
        };
        let map = self.user_ctx.clone();

        uniffi_async(async move {
            user_context.sign_out_all().await?;
            map.clear();

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }

    /// What aditional protection of the App is configured.
    ///
    pub async fn app_protection(&self) -> Result<AppProtection, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let tether = ctx.core_context().account_stash().connection().await?;
            let app_settings = RealAppSettings::get_or_default(&tether).await;

            Result::<_, RealProtonMailError>::Ok(app_settings.protection.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Start the auto lock countdown.
    ///
    /// This method is meant to be used when app is about to be put in the background.
    /// It will start the auto lock countdown and will be used to determine if the app
    /// spent enough time in the background to be locked.
    ///
    pub fn start_auto_lock_countdown(&self) {
        self.mail_ctx.core_context().clock().auto_lock_tick();
    }

    /// Shoul invoke app lock according to app settings.
    ///
    /// Method will update itself to new access time when returning `true`,
    /// It assumes client will invoke the app protection by themself
    ///
    pub async fn should_auto_lock(&self) -> Result<bool, UserSessionError> {
        let ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            let tether = ctx.core_context().account_stash().connection().await?;
            let app_settings = RealAppSettings::get_or_default(&tether).await;

            Result::<_, RealProtonMailError>::Ok(app_settings.should_auto_lock(ctx.core_context()))
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Mark biometrics check as passed.
    ///
    /// This method is used to mark that the biometrics check has been passed
    /// for autolock to reset the timer.
    ///
    pub fn biometrics_check_passed(&self) {
        self.mail_ctx.core_context().clock().auto_lock_accessed();
    }

    /// Create a PIN App protection.
    ///
    /// The same PIN will be required for authentication of the user
    /// or when user want to change the way of authentication in the App.
    ///
    pub async fn set_pin_code(&self, pin: Vec<u32>) -> Result<(), PinSetError> {
        let ctx = self.mail_ctx.core_context().clone();

        uniffi_async(async move {
            PinCode::set(ctx, pin).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(PinSetError::from)
    }

    /// Authenticate stored PIN
    ///
    pub async fn verify_pin_code(&self, pin: Vec<u32>) -> Result<(), PinAuthError> {
        let mail_ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            mail_ctx
                .verify_pin_code(pin)
                .await
                .map_err(RealProtonMailError::from)
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
        let mail_ctx = self.mail_ctx.clone();

        uniffi_async(async move {
            mail_ctx
                .delete_pin_code(pin)
                .await
                .map_err(RealProtonMailError::from)
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
            let tether = ctx.account_stash().connection().await?;
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
            let mut tether = ctx.account_stash().connection().await?;
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
            let mut tether = ctx.account_stash().connection().await?;
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
            let tether = ctx.account_stash().connection().await?;
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
            let mut tether = ctx.account_stash().connection().await?;
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

    /// When the flow is considered logged in, transform it into a `MailUserSession`.
    pub async fn to_user_session(
        &self,
        ffi_flow: Arc<LoginFlow>,
    ) -> Result<Arc<MailUserSession>, UserSessionError> {
        self.params.origin.guard(Origin::App)?;

        let ctx = self.mail_ctx.clone();
        let user_ctxs = self.user_ctx.clone();

        uniffi_async(async move {
            let core_flow = ffi_flow.inner_flow();
            let mut guard = core_flow.lock().await;
            ctx.user_context_from_login_flow(&mut guard)
                .map_ok(|ctx| user_ctxs.insert(ctx))
                .map_ok(MailUserSession::new)
                .map_err(RealProtonMailError::from)
                .await
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Converts this session into a [`MailUserSession`] for the primary user.
    ///
    /// This is meant to be used only within extensions - in particular, it
    /// assumes that the primary user is already logged in.
    pub async fn to_primary_user_session(&self) -> Result<Arc<MailUserSession>, UserSessionError> {
        self.params.origin.guard(Origin::IosShareExt)?;

        let ctx = self.mail_ctx.clone();
        let user_ctxs = self.user_ctx.clone();

        uniffi_async(async move {
            let Some(account) = ctx
                .core_context()
                .get_primary_account()
                .await
                .map_err(MailContextError::from)?
            else {
                return Err(RealProtonMailError::Reason(MailErrorReason::ContextReason(
                    ContextErrorReason::UserContextNotInitialized(
                        "Primary account not found".into(),
                    ),
                )));
            };

            debug!(
                id=?account.remote_id,
                "Primary account found, looking for the primary session",
            );

            let mut primary_session = None;

            let sessions = ctx
                .core_context()
                .get_account_sessions(account.remote_id.clone())
                .await
                .map_err(MailContextError::from)?;

            for session in sessions {
                debug!(id=?session.remote_id, "Checking session");

                let Ok(Some(_)) = ctx
                    .core_context()
                    .get_session_state(session.remote_id.clone())
                    .await
                else {
                    continue;
                };

                primary_session = Some(session);
                break;
            }

            let Some(primary_session) = primary_session else {
                warn!("Couldn't find primary session");

                return Err(RealProtonMailError::Reason(MailErrorReason::ContextReason(
                    ContextErrorReason::UserContextNotInitialized(
                        "Primary session not found".into(),
                    ),
                )));
            };

            debug!(
                id=?primary_session.remote_id,
                "Primary session found, looking for the context",
            );

            let user_ctx = ctx
                .initialized_user_context_from_session(&primary_session)
                .await?;

            let Some(user_ctx) = user_ctx else {
                warn!(
                    session_id=?primary_session.remote_id,
                    "Couldn't find initialized context for primary session",
                );

                return Err(RealProtonMailError::Reason(MailErrorReason::ContextReason(
                    ContextErrorReason::UserContextNotInitialized(
                        "Primary session's context is not initialized".into(),
                    ),
                )));
            };

            let ctx = user_ctxs.insert(user_ctx);

            Ok::<_, RealProtonMailError>(MailUserSession::new(ctx))
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

    /// Logs out all sessions of an account and deletes the account's data.
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
    #[returns(VoidSessionResult)]
    pub async fn logout_account(&self, user_id: String) -> Result<(), UserSessionError> {
        let mail_ctx = self.mail_ctx.clone();
        let user_ctx = self.user_ctx.clone();
        let user_id = user_id.into();

        uniffi_async(async move {
            if user_ctx.remove(&user_id).is_none() {
                debug!("Logging out account without any active context");
            }

            mail_ctx
                .logout_account_and_delete_user_data(user_id)
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

    pub fn on_enter_foreground(&self) {
        self.ctx().core_context().on_enter_foreground();
    }

    pub fn on_exit_foreground(&self) {
        async_runtime().block_on(async {
            self.ctx().core_context().on_exit_foreground().await;
        });
    }

    /// Export all logs into a single file wih the given `file_path`
    ///
    /// Returns the number of bytes written.
    pub fn export_logs(&self, file_path: String) -> Result<u64, ProtonError> {
        let path = PathBuf::from(file_path);
        self.ctx()
            .core_context()
            .log_service()
            .export_logs(&path)
            .map_err(|e| {
                error!("Failed to export logs: {e:?}");
                ProtonError::Unexpected(UnexpectedError::Os)
            })
            .map(|v| v as u64)
    }

    /// Is the Unleash feature enabled.
    /// Currently:
    /// * Returns None if feature is not found
    /// * Returns Some(true) if feature is present
    ///
    /// NOTE: It never returns Some(false) as in this stage of the implementation.
    pub async fn is_feature_enabled(&self, feature_id: String) -> Option<bool> {
        self.mail_ctx.feature_flags().get(&feature_id).await
    }

    pub fn update_os_network_status(&self, os_network_status: OsNetworkStatus) {
        self.mail_ctx
            .network_monitor_service()
            .update_os_network_status(os_network_status.into());
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
        ctx: &MailContext,
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel(ctx, handle, callback))
    }

    fn new_async(
        ctx: &MailContext,
        accounts: Vec<Arc<StoredAccount>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedAccounts {
        WatchedAccounts::new(accounts, watch_channel_async(ctx, handle, callback))
    }
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsNetworkStatus {
    Online,
    Offline,
}

impl From<OsNetworkStatus> for RealOsNetworkStatus {
    fn from(status: OsNetworkStatus) -> Self {
        match status {
            OsNetworkStatus::Online => Self::Online,
            OsNetworkStatus::Offline => Self::Offline,
        }
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
        ctx: &MailContext,
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Box<dyn LiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel(ctx, handle, callback))
    }

    fn new_async(
        ctx: &MailContext,
        sessions: Vec<Arc<StoredSession>>,
        handle: WatcherHandle,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> WatchedSessions {
        WatchedSessions::new(sessions, watch_channel_async(ctx, handle, callback))
    }
}
