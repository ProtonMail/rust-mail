//! Core context contains all the necessary information to retrieve or create new accounts and sessions.

mod builder;
mod registry;
pub mod services;
use registry::ServiceRegistry;
use services::logging_service::LoggingService;
use tokio::runtime;

use crate::action_queue::CoreActionError;
use crate::app_events::{OnEnterForegroundEvent, OnExitForegroundEvent};
use crate::auth_store::{AuthStore, DecryptExt};
use crate::core_clock::CoreClock;
use crate::datatypes::{
    ApiConfig, LocalContactId, StoredDevicePrivateKey, StoredDevicePublicKey, TfaStatus,
};
use crate::db::account::{CoreAccount, CoreSession, SessionEncryptionKey};
use crate::db::migrations::{migrate_account_db, verify_account_db};
use crate::device::DynDeviceInfoProvider;
use crate::event_loop::EventPollMode;
use crate::models::{AppSettings, ModelExtension};
use crate::nuke_utils::{
    drop_all_tables_in_database, remove_or_clear_dir_safe, rename_database_files,
};
use crate::os::{KeyChain, KeyChainError, KeyChainExt, StoreInKeyChain};
use crate::pin_code::PinCode;
use crate::services::issue_reporter_service::IssueReporterService;
use crate::services::{ContextEventService, NetworkMonitorService};
use crate::{KeyHandlingError, UserContext, UserDatabaseInitializer};
use anyhow::{Context as _, Error as AnyhowError, anyhow};
use async_trait::async_trait;
pub use builder::ContextBuilder;
use futures::TryFutureExt;
use itertools::Itertools;
use proton_action_queue::action::{self, Action, WriterGuardError};
use proton_action_queue::queue::{
    ActionError as QueueActionError, ActionRequeueReason, QueuedError,
};
use proton_core_api::auth::{Auth, Tokens};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::muon::client::{Fingerprint, InfoProvider};
use proton_core_api::services::proton::{BuildError, PrivateEmail};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::Config as RealApiConfig;
use proton_core_api::session::Session as ApiSession;
use proton_core_api::store::{MbpMode, Store, TempStore, UserData};
use proton_core_api::verification::DynChallengeNotifier;
use proton_crypto_account::keys::PGPDeviceKey;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_event_loop::EventLoopError;
use proton_issue_reporter_service::{IssueLevel, IssueReporter, issue_report_keys_from_error};
use proton_log_service::LogService;
use proton_network_monitor_service::{ConnectionMonitor, NetworkMonitorServiceError};
use proton_sqlite3::MigratorError;
use proton_task_service::{BackgroundAwareTaskService, TaskService};
use proton_task_service::{Spawner, SpawnerRef};
use proton_vcard::VcardValidationError;
use secrecy::{ExposeSecret, SecretVec};
use serde_json::json;
use services::{
    DeviceInfoService, EventPollConfigService, HvNotifierService, SessionObserverService,
};
use stash::orm::Model as _;
use stash::stash::{Stash, StashConfiguration, StashError, WatcherHandle};
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum CoreContextError {
    #[error("Account with user id {0} is missing in the DB")]
    AccountMissing(UserId),
    #[error("Settings for user with id {0} are missing in the DB")]
    SettingsMissing(UserId),
    #[error("Build error: {0}")]
    Build(#[from] BuildError),
    #[error("API error: {0}")]
    Api(#[from] ApiServiceError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("Action: {0}")]
    Action(#[from] CoreActionError),
    #[error("QueuedAction: {0}")]
    QueuedAction(#[from] QueuedError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::queue::Error),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] MigratorError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(#[from] KeyHandlingError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Problem with loading contact: {0}")]
    ContactError(#[from] ContactError),
    #[error("Attempting to create more than one context for the user with id {0}")]
    DuplicateContext(UserId),
    #[error("Queue Writer Guard Expired")]
    QueueWriterGuardExpired,
    #[error(transparent)]
    NetworkMonitorService(#[from] NetworkMonitorServiceError),
    #[error("{0}")]
    Other(#[from] AnyhowError),
}

impl<T: Action<Error: Into<CoreContextError>>> From<QueueActionError<T>> for CoreContextError {
    fn from(value: QueueActionError<T>) -> Self {
        match value {
            QueueActionError::Action(e) => e.into(),
            QueueActionError::Queue(e) => Self::ActionQueue(e),
        }
    }
}

impl From<WriterGuardError> for CoreContextError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => CoreContextError::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => CoreContextError::Stash(e),
        }
    }
}

impl From<VcardValidationError> for CoreContextError {
    fn from(e: VcardValidationError) -> Self {
        CoreContextError::ContactError(ContactError::Validation(e))
    }
}

impl From<JoinError> for CoreContextError {
    fn from(e: JoinError) -> Self {
        CoreContextError::Other(anyhow!(e))
    }
}

impl action::Error for CoreContextError {
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        match self {
            Self::Api(e) if e.is_network_failure() => Some(ActionRequeueReason::NetworkFailed),
            Self::QueueWriterGuardExpired => Some(ActionRequeueReason::GuardExpired),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("ContactCard not found for email: {0}")]
    CardNotFound(PrivateEmail),
    #[error("RemoteId not present for ContactCard for email: {0}")]
    ContactCardRemoteIdNotPresent(PrivateEmail),
    #[error("Contact not found for email: {0}")]
    FullContactNotFound(PrivateEmail),
    #[error("Validation: {0}")]
    Validation(#[from] VcardValidationError),
    #[error("Contact {0} does not have remote id")]
    ContactDoesNotHaveRemoteId(LocalContactId),
}

/// Represents the state of an account.
#[derive(Debug, Clone)]
pub enum CoreAccountState {
    /// The account is not yet ready to be used.
    NotReady,

    /// The account has at least one fully logged-in session;
    /// the variant holds the (remote) IDs of the fullly logged-in sessions.
    LoggedIn(Vec<SessionId>),

    /// The account has authenticated sessions but they are missing the key secret.
    /// The variant holds the (remote) IDs of the sessions that are missing the key secret.
    NeedMbp(Vec<SessionId>),

    /// The account has partially authenticated sessions that require a second factor.
    /// The variant holds the (remote) IDs of the sessions that require a second factor.
    NeedTfa(Vec<SessionId>),

    /// The account has a temporary password that must be set before it can be used.
    /// The variant holds the (remote) IDs of the sessions that require a new password.
    NeedNewPass(Vec<SessionId>),

    /// The account has no active sessions.
    LoggedOut,
}

impl CoreAccountState {
    pub fn of(account: &CoreAccount, sessions: &[CoreSession]) -> Self {
        // Group sessions by state.
        let mut sessions_by_state = (sessions.iter())
            .map(|session| (CoreSessionState::of(session), session.remote_id.clone()))
            .into_group_map();

        // Does the account have a temporary password?
        if account.temp_pass {
            return CoreAccountState::NeedNewPass(sessions_by_state.into_values().concat());
        }

        // Does the account have any fully authenticated sessions?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::Authenticated) {
            return CoreAccountState::LoggedIn(sessions);
        }

        // Does the account have any sessions that are awaiting a mailbox password?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::NeedKey) {
            // Now that the password_mode is set in a later step with the /settings call
            // We can't rely anymore on this check since it will always be false
            // if account.password_mode.is_some_and(PasswordMode::has_mbp) {
            return CoreAccountState::NeedMbp(sessions);
            // }
        }

        // Does the account have any sessions that are awaiting a second factor?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::NeedTfa)
            && account.second_factor_mode.is_some_and(TfaStatus::has_tfa)
        {
            return CoreAccountState::NeedTfa(sessions);
        }

        // Is the account ready for use?
        if account.is_ready {
            return CoreAccountState::LoggedOut;
        }

        CoreAccountState::NotReady
    }
}

/// Represents the state of a session.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum CoreSessionState {
    /// The session is fully authenticated and ready to use.
    Authenticated,

    /// The session has authenticated but is missing the key secret.
    NeedKey,

    /// The session has partially authenticated and requires a second factor.
    NeedTfa,
}

impl CoreSessionState {
    #[must_use]
    pub fn of(session: &CoreSession) -> Self {
        if session.auth_scopes.contains("twofactor") {
            CoreSessionState::NeedTfa
        } else if session.key_secret.is_none() {
            CoreSessionState::NeedKey
        } else {
            CoreSessionState::Authenticated
        }
    }
}

pub type CoreContextResult<T> = Result<T, CoreContextError>;

#[allow(dead_code)]
pub struct Context {
    this: Weak<Self>,
    active_user_contexts: Mutex<HashMap<UserId, Weak<UserContext>>>,
    // Data
    origin: Origin,
    user_db_path: PathBuf,
    account_db_path: PathBuf,
    cache_path: PathBuf,
    // Configuration
    api_config: ApiConfig,
    // Essential services
    account_stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    cancellation_token: CancellationToken,
    user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
    task_service: BackgroundAwareTaskService,
    service_registry: ServiceRegistry<CoreContextError>,
}

impl std::ops::Deref for Context {
    type Target = ServiceRegistry<CoreContextError>;

    fn deref(&self) -> &Self::Target {
        &self.service_registry
    }
}

const SESSION_OBSERVER_BROADCAST_CAPACITY: usize = 8;

impl Context {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mut builder: ContextBuilder,
        origin: Origin,
        runtime: runtime::Handle,
        account_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        initializers: Vec<Box<dyn UserDatabaseInitializer>>,
        api_config: ApiConfig,
        hv_notifier: Option<DynChallengeNotifier>,
        device_info_provider: Option<DynDeviceInfoProvider>,
        cache_path: impl Into<PathBuf>,
        log_service: LogService,
        event_poll_mode: EventPollMode,
        network_monitor_config: proton_network_monitor_service::Config,
        issue_reporter: Arc<dyn IssueReporter>,
    ) -> CoreContextResult<Arc<Self>> {
        let issue_reporter_cloned = issue_reporter.clone();
        async {
            let account_db_path = account_db_path.into();
            let user_db_path = user_db_path.into();

            match origin {
                Origin::App => {
                    fs::create_dir_all(&account_db_path)?;
                    fs::create_dir_all(&user_db_path)?;
                }

                Origin::ShareExt => {
                    if !account_db_path.exists() {
                        return Err(anyhow!(
                            "Account database not found: {}",
                            account_db_path.display()
                        )
                        .into());
                    }

                    if !user_db_path.exists() {
                        return Err(
                            anyhow!("User database not found: {}", user_db_path.display()).into(),
                        );
                    }
                }
            }

            let account_stash_path = get_account_db_path(&account_db_path);

            let stash_config = StashConfiguration {
                path: Some(&account_stash_path),
                pool_size: Some(24),
                ..Default::default()
            };

            let account_stash = Stash::new(stash_config)?;

            match origin {
                Origin::App => {
                    migrate_account_db(&account_stash).await?;
                }
                Origin::ShareExt => {
                    verify_account_db(&account_stash).await?;
                }
            }

            let task_service = TaskService::new(runtime)?;
            let background_task_service = BackgroundAwareTaskService::new(task_service);

            builder = builder
                .with_service(CoreClock::default())
                .with_service(LoggingService::new(log_service))
                .with_service(ContextEventService::new())
                .with_service(IssueReporterService::new(issue_reporter))
                .with_cyclic_service(|ctx| NetworkMonitorService::new(ctx, network_monitor_config));

            if matches!(origin, Origin::App) {
                builder = builder
                    .with_cyclic_service(|weak_ctx| {
                        SessionObserverService::new(weak_ctx, SESSION_OBSERVER_BROADCAST_CAPACITY)
                    })
                    .with_service(HvNotifierService::new(hv_notifier))
                    .with_service(DeviceInfoService::new(device_info_provider))
                    .with_service(EventPollConfigService::new(event_poll_mode));
            }

            builder
                .build(
                    origin,
                    user_db_path,
                    account_db_path,
                    cache_path.into(),
                    api_config,
                    account_stash,
                    key_chain,
                    initializers,
                    background_task_service,
                )
                .await
        }
        .await
        .inspect_err(|e| {
            issue_reporter_cloned.report(
                IssueLevel::Critical,
                "Failed to create core context".into(),
                issue_report_keys_from_error(e),
            );
        })
    }

    #[must_use]
    pub fn as_arc(&self) -> Arc<Self> {
        self.this.upgrade().expect("Should never fail")
    }

    #[must_use]
    pub fn as_weak(&self) -> Weak<Self> {
        Weak::clone(&self.this)
    }

    #[must_use]
    pub fn spawner(&self) -> SpawnerRef {
        SpawnerRef::new(self.as_weak())
    }

    #[must_use]
    pub fn origin(&self) -> Origin {
        self.origin
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
    pub async fn get_accounts(&self) -> CoreContextResult<Vec<CoreAccount>> {
        let tether = self.account_stash().connection().await?;
        Ok(CoreAccount::all(&tether).await?)
    }

    /// Watch the accounts for changes.
    ///
    /// # Returns
    ///
    /// Returns a tuple containing the initial list of accounts and a receiver for changes.
    /// The receiver is a channel over which change events are sent, such as when a new account is created,
    /// an existing account is updated, or an account is deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_accounts(&self) -> CoreContextResult<(Vec<CoreAccount>, WatcherHandle)> {
        let accounts = self.get_accounts().await?;
        let handle = CoreAccount::watch(self.account_stash())?;

        Ok((accounts, handle))
    }

    /// Get all available API sessions.
    ///
    /// A session represents an authenticated session with the Proton API for a given account,
    /// including the authentication tokens granted by the API, the state of the session,
    /// and the user's key passphrase (once known).
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_sessions(&self) -> CoreContextResult<Vec<CoreSession>> {
        let tether = self.account_stash().connection().await?;
        Ok(CoreSession::all(&tether).await?)
    }

    /// Get all authenticated API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    ///
    /// # Returns
    ///
    /// Returns an iterator of authenticated sessions.
    ///
    /// More details on authenticated sessions can be found in the
    /// [`get_sessions`] documentation.
    ///
    pub async fn get_authenticated_sessions(
        &self,
    ) -> CoreContextResult<impl Iterator<Item = CoreSession>> {
        let sessions = self.get_sessions().await?;
        Ok(sessions
            .into_iter()
            .filter(|s| CoreSessionState::of(s) == CoreSessionState::Authenticated))
    }

    /// Check if any account is logged in.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    ///
    pub async fn any_logged_in_account(&self) -> CoreContextResult<bool> {
        let sessions = self.get_authenticated_sessions().await?;
        Ok(sessions.count() > 0)
    }

    /// Client ID is a string that uniquely identifies the client application without the version number.
    /// Example: "ios-mail"
    #[must_use]
    pub fn get_client_id(&self) -> String {
        self.api_config.get_client_id()
    }

    /// Watch the API sessions for changes.
    ///
    /// # Returns
    ///
    /// Returns a tuple containing the initial list of sessions and a receiver for changes.
    /// The receiver is a channel over which change events are sent, such as when a new session is created,
    /// an existing session is updated, or a session is deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_sessions(&self) -> CoreContextResult<(Vec<CoreSession>, WatcherHandle)> {
        let tether = self.account_stash().connection().await?;
        let sessions = CoreSession::all(&tether).await?;
        let handle = CoreSession::watch(self.account_stash())?;

        Ok((sessions, handle))
    }

    /// Get all API sessions associated with a given account.
    ///
    /// See [`Context::get_sessions`] for more information on API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_account_sessions(
        &self,
        user_id: UserId,
    ) -> CoreContextResult<Vec<CoreSession>> {
        let tether = self.account_stash().connection().await?;
        Ok(CoreSession::find_by_user_id(user_id, &tether).await?)
    }

    /// Watch an account's API sessions for changes.
    ///
    /// See [`Context::watch_sessions`] for more information on watching API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_account_sessions(
        // TODO: Two types of watchers on session, it needs to be unified.
        &self,
        user_id: UserId,
    ) -> CoreContextResult<(Vec<CoreSession>, WatcherHandle)> {
        let sessions = self.get_account_sessions(user_id).await?;
        let handle = CoreSession::watch(self.account_stash())?;

        Ok((sessions, handle))
    }

    /// Get a single account by its remote (user) ID.
    ///
    /// This is a convenience method that enables retrieving a single account without requiring
    /// the full set of accounts to be loaded first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account(&self, user_id: UserId) -> CoreContextResult<Option<CoreAccount>> {
        let tether = self.account_stash().connection().await?;
        Ok(CoreAccount::find_by_id(user_id, &tether).await?)
    }

    /// Get the login state of an account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account_state(
        &self,
        user_id: UserId,
    ) -> CoreContextResult<Option<CoreAccountState>> {
        let tether = self.account_stash().connection().await?;
        let Some(account) = CoreAccount::find_by_id(user_id.clone(), &tether).await? else {
            return Ok(None);
        };

        let state = CoreSession::find_by_user_id(user_id, &tether)
            .map_ok(|s| CoreAccountState::of(&account, &s))
            .await?;

        Ok(Some(state))
    }

    /// Get a single API session by its associated session ID.
    ///
    /// This is a convenience method that enables retrieving a single session without requiring
    /// the full set of sessions to be loaded first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session(
        &self,
        session_id: SessionId,
    ) -> CoreContextResult<Option<CoreSession>> {
        let tether = self.account_stash().connection().await?;
        Ok(CoreSession::find_by_id(session_id, &tether).await?)
    }

    /// Get the login state of a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session_state(
        &self,
        session_id: SessionId,
    ) -> CoreContextResult<Option<CoreSessionState>> {
        let tether = self.account_stash().connection().await?;
        let Some(session) = CoreSession::find_by_id(session_id, &tether).await? else {
            return Ok(None);
        };

        Ok(Some(CoreSessionState::of(&session)))
    }

    /// Get the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_primary_account(&self) -> CoreContextResult<Option<CoreAccount>> {
        let tether = self.account_stash().connection().await?;
        for account in CoreAccount::by_primary_seq(&tether).await? {
            let Some(state) = self.get_account_state(account.remote_id.clone()).await? else {
                continue;
            };

            if let CoreAccountState::LoggedIn(_) = state {
                return Ok(Some(account));
            }
        }

        Ok(None)
    }

    /// Set the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the account is not found.
    pub async fn set_primary_account(&self, user_id: UserId) -> CoreContextResult<()> {
        let mut tether = self.account_stash().connection().await?;

        let seq_max = CoreAccount::primary_seq_max(&tether).await?;

        let account = CoreAccount::find_by_id(user_id.clone(), &tether)
            .await?
            .ok_or(CoreContextError::AccountMissing(user_id))?;

        tether
            .tx(async |tx| account.with_primary_seq(seq_max + 1).save(tx).await)
            .await?;

        Ok(())
    }

    /// Get a user context from an existing session.
    ///
    /// # Errors
    ///
    /// TODO: Document errors
    ///
    pub async fn user_context_from_session(
        &self,
        session: &CoreSession,
    ) -> CoreContextResult<Arc<UserContext>> {
        // Ensure we have an encryption key
        let key = self.get_encryption_key()?;

        // Ensure the key can be used to decrypt the access token
        let _ = session
            .access_token
            .decrypt_to_string(&key)
            .inspect_err(|_| tracing::error!("Could not decrypt access token"))
            .or(Err(CoreContextError::Crypto))?;

        // Ensure the key can be used to decrypt the refresh token
        let _ = session
            .refresh_token
            .decrypt_to_string(&key)
            .inspect_err(|_| tracing::error!("Could not decrypt refresh token"))
            .or(Err(CoreContextError::Crypto))?;

        let user_id = session.account_id.clone();
        let session_id = session.remote_id.clone();
        let session = self
            .new_api_session(Some(session))
            .await
            .inspect_err(|e| tracing::error!("Could not create api session: {e:?}"))?;

        self.new_user_context(user_id, session, session_id).await
    }

    /// Logs out all sessions of an account without deleting the account's data.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn logout_account(&self, user_id: UserId) -> CoreContextResult<()> {
        for session in self.get_account_sessions(user_id.clone()).await? {
            let Ok(api) = self
                .new_api_session(Some(&session))
                .inspect_err(|err| error!("failed to create API session: {err:?}"))
                .await
            else {
                continue;
            };

            let Ok(()) = api
                .logout()
                .inspect_err(|err| error!("failed to logout API session: {err:?}"))
                .await
            else {
                continue;
            };
            info!("logged out session {}", session.remote_id);
        }

        let orphaned_sessions = self
            .get_account_sessions(user_id.clone())
            .await?
            .into_iter()
            .map(|s| s.remote_id)
            .collect::<Vec<_>>();

        if !orphaned_sessions.is_empty() {
            warn!(
                "Orphaned sessions found in database: {:?}",
                orphaned_sessions
            );

            let mut tether = self.account_stash().connection().await?;
            tether
                .tx(async |tx| CoreSession::delete_by_ids(orphaned_sessions, tx).await)
                .await?;
        }

        if let Ok(false) = self.any_logged_in_account().await {
            tracing::debug!("Remove any remaining app protection settings");
            PinCode::force_delete(self.as_arc())
                .await
                .map_err(|e| anyhow!("Could not remove PIN, details: `{e}`"))?;
        }

        info!("logged out all sessions for account {user_id}");

        Ok(())
    }

    /// Log out and delete all associated user data.
    ///
    /// Unlike [`delete_account()`] it preserve the account metadata so that it still available
    /// from the session picker.
    ///
    /// ### Notes
    ///
    ///  Function assumes separate database files for
    /// `Account` and `User` databases
    ///
    #[tracing::instrument(skip(self, caches))]
    pub async fn logout_and_delete_user_data(
        &self,
        user_id: UserId,
        caches: Vec<PathBuf>,
    ) -> CoreContextResult<()> {
        tracing::info!("Kill all background tasks for this user");
        self.cancel_user_tasks(&user_id).await;

        let session = self
            .get_account_sessions(user_id.clone())
            .await?
            .into_iter()
            .find(|session| CoreSessionState::Authenticated == CoreSessionState::of(session));

        if let Some(session) = session {
            tracing::info!("Clear all user data from database");
            if let Ok(user_ctx) = self.user_context_from_session(&session).await {
                let tether = user_ctx.stash().connection().await?;

                if let Err(e) = drop_all_tables_in_database(tether).await {
                    tracing::error!("Could not clean user database, details: `{e}`");
                }
            }
        }

        tracing::info!("Logout user");
        if let Err(e) = self.logout_account(user_id.clone()).await {
            tracing::error!("Could not logout account, details: `{e}`");
        }

        tracing::info!("Remove user from active_contexts");
        self.active_user_contexts.lock().await.remove(&user_id);

        tracing::info!("Archive & try to remove user database");
        let user_db_location = self.user_db_path(&user_id);
        rename_database_files(&user_db_location).await;
        remove_or_clear_dir_safe(&user_db_location).await;

        tracing::info!("Clear user associated caches");
        for cache_path in caches {
            remove_or_clear_dir_safe(cache_path).await;
        }
        Ok(())
    }

    /// Logs out and removes an account, dropping associated data from user
    /// database and renaming empty database file to include `.nuked` extension,
    /// after which try to remove any remaining files from the hard drive
    /// including archived databases and supplied caches.
    ///
    /// ### Notes
    ///
    /// Unlike [`logout_and_delete_user_data()`] it does not preserve the account metadata, and it
    /// will no longer be available in the session picker.
    ///
    ///  Function assumes separate database files for
    /// `Account` and `User` databases
    ///
    #[tracing::instrument(skip(self, caches))]
    pub async fn delete_account(
        &self,
        user_id: UserId,
        caches: Vec<PathBuf>,
    ) -> CoreContextResult<()> {
        self.logout_and_delete_user_data(user_id.clone(), caches)
            .await?;

        tracing::info!("Remove account");
        let mut tether = self.account_stash().connection().await?;
        tether
            .tx(async |tx| {
                CoreAccount::delete_by_id(user_id, tx)
                    .await
                    .inspect_err(|e| tracing::error!("Failed to delete account from db: {e:?}"))
            })
            .await?;

        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    #[allow(clippy::result_large_err)]
    pub fn get_encryption_key(&self) -> CoreContextResult<SessionEncryptionKey> {
        let Some(key) = self.load_secret::<SessionEncryptionKey>()? else {
            return Err(CoreContextError::KeyChainHasNoKey);
        };

        Ok(key)
    }

    /// Creates a new pair of public and private device keys, used for decrypting and encrypting
    /// push notifications.
    ///
    /// It stores the private part in the key chain.
    ///
    /// # Errors
    ///
    /// It may return an error if crypto operation fails or if it fails to store key in the keychain.
    ///
    #[allow(clippy::result_large_err)]
    pub fn gen_device_key_pair<P>(&self, pgp: &P) -> CoreContextResult<StoredDevicePublicKey>
    where
        P: PGPProviderSync,
    {
        let key = PGPDeviceKey::generate(pgp).map_err(|_| CoreContextError::Crypto)?;

        let private_key = key
            .serialize_to_secure_storage(pgp)
            .map_err(|_| CoreContextError::Crypto)?;

        let private_key = StoredDevicePrivateKey::with_bytes(private_key.as_bytes().to_vec());

        let public_key = key
            .export_public_key(pgp)
            .map_err(|_| CoreContextError::Crypto)?;

        self.key_chain
            .store::<StoredDevicePrivateKey>(private_key)?;

        Ok(StoredDevicePublicKey::from(public_key))
    }

    pub fn store_secret<S: StoreInKeyChain>(&self, secret: S) -> Result<(), KeyChainError> {
        self.key_chain.store::<S>(secret)
    }

    pub fn load_secret<S: StoreInKeyChain>(&self) -> Result<Option<S>, KeyChainError> {
        self.key_chain.load::<S>()
    }

    pub fn delete_secret<S: StoreInKeyChain>(&self) -> Result<(), KeyChainError> {
        self.key_chain.delete::<S>()
    }

    pub(crate) fn user_db_path(&self, user_id: &UserId) -> PathBuf {
        get_user_db_path(&self.user_db_path, user_id)
    }

    pub async fn new_api_session(
        &self,
        session: Option<&CoreSession>,
    ) -> CoreContextResult<ApiSession> {
        match self.origin {
            Origin::App => self.new_api_session_app(session).await,
            Origin::ShareExt => self.new_api_session_ext(session).await,
        }
    }

    async fn new_api_session_app(
        &self,
        session: Option<&CoreSession>,
    ) -> CoreContextResult<ApiSession> {
        let user_id = session.map(|s| &s.account_id).cloned();
        let session_id = session.map(|s| &s.remote_id).cloned();
        let account_stash = self.account_stash().to_owned();
        let keychain = Arc::clone(&self.key_chain);
        let store = AuthStore::new(account_stash, keychain, user_id, session_id);
        let api_config = RealApiConfig::from(self.api_config.clone());
        let app_settings =
            AppSettings::get_or_default(&self.account_stash().connection().await?).await;

        let network_monitor_service = self.get_service::<NetworkMonitorService>();

        let mut builder = ApiSession::builder()
            .with_config(api_config)
            .with_store(store)
            .with_connection_monitor(network_monitor_service.new_connection_monitor())
            .with_allow_doh(app_settings.use_alternative_routing);

        if let Some(hv_service) = self.get_service_opt::<HvNotifierService>()
            && let Some(notifier) = hv_service.notifier_arc()
        {
            builder = builder.with_notifier(notifier);
        }

        if let Some(device_service) = self.get_service_opt::<DeviceInfoService>()
            && let Some(provider) = device_service.provider()
        {
            builder = builder.with_info_provider(Arc::new(MuonInfoProvider {
                app_version: RealApiConfig::from(self.api_config.clone()).app_version,
                device_info_provider: Arc::clone(provider),
            }));
        }

        Ok(builder.build().await?)
    }

    async fn new_network_monitor_api_session(
        &self,
        connection_monitor: ConnectionMonitor,
    ) -> CoreContextResult<ApiSession> {
        let api_config = RealApiConfig::from(self.api_config.clone());
        let app_settings =
            AppSettings::get_or_default(&self.account_stash().connection().await?).await;

        let mut builder = ApiSession::builder()
            .with_config(api_config)
            .with_connection_monitor(connection_monitor)
            .with_allow_doh(app_settings.use_alternative_routing);

        if let Some(hv_service) = self.get_service_opt::<HvNotifierService>()
            && let Some(notifier) = hv_service.notifier_arc()
        {
            builder = builder.with_notifier(notifier);
        }

        if let Some(device_service) = self.get_service_opt::<DeviceInfoService>()
            && let Some(provider) = device_service.provider()
        {
            builder = builder.with_info_provider(Arc::new(MuonInfoProvider {
                app_version: RealApiConfig::from(self.api_config.clone()).app_version,
                device_info_provider: Arc::clone(provider),
            }));
        }

        Ok(builder.build().await?)
    }

    async fn new_api_session_ext(
        &self,
        session: Option<&CoreSession>,
    ) -> CoreContextResult<ApiSession> {
        let session = session.context("Missing core session")?;
        let user_id = session.account_id.clone();
        let session_id = session.remote_id.clone();

        let key = self
            .key_chain
            .load::<SessionEncryptionKey>()?
            .context("Missing session encryption key")?;

        let tokens = {
            let acc_tok = session.access_token.decrypt_to_string(&key)?;
            // There is a risk of race condition when refreshing the token.
            // Since this share extension is used in a separate process, and main process may be still alive,
            // there is a risk that both processes may try to refresh the token at the same time.
            // In that case, only one of them would win, and the Backend would return an error for the other one.
            // If Share Extension wins, then main application would fail, causing user to log out.
            // To avoid it, we ensure that this refresh token is invalid - it is better to just get a failure here,
            // return a message "please try again" or "Session expired, please log in again" - than to log out the main
            // application.
            // Therefore, this refresh token is purposefully invalid - empty.
            let ref_tok = "";
            let scopes = session.auth_scopes.clone().into_inner();

            Tokens::access(acc_tok.expose_secret(), ref_tok, scopes)
        };

        let auth = Auth::internal(
            user_id.clone().into_inner(),
            session_id.clone().into_inner(),
            tokens,
        );

        let account_stash = self.account_stash().to_owned();
        let keychain = Arc::clone(&self.key_chain);
        // WARNING: make sure you are not actually using the store in any muon client here.
        // We use it only to get key secret in convenient way.
        let db_store = AuthStore::new(
            account_stash,
            keychain,
            Some(user_id.clone()),
            Some(session_id),
        );
        let key_secret = db_store
            .expose_key_secret()
            .await
            .context("Missing key secret")?;
        let store = {
            let account = self
                .get_account(user_id)
                .await?
                .context("Missing account")?;

            let mut store = TempStore::boxed();
            store
                .set_user_data(UserData {
                    username: account.username.context("Missing username")?,
                    display_name: account.display_name.context("Missing display name")?,
                    primary_addr: account.primary_addr.context("Missing primary address")?,
                    password_mode: account.password_mode.map_or(MbpMode::One, Into::into),
                    key_secret,
                })
                .await?;
            store.set_auth(auth).await?;
            store
        };

        let network_monitor_service = self.get_service::<NetworkMonitorService>();

        let app_settings =
            AppSettings::get_or_default(&self.account_stash().connection().await?).await;

        let builder = ApiSession::builder()
            .with_config(RealApiConfig::from(self.api_config.clone()))
            .with_store(store)
            .with_connection_monitor(network_monitor_service.new_connection_monitor())
            .with_allow_doh(app_settings.use_alternative_routing);

        let primary_session = builder.build().await?;

        let forked_session = primary_session
            .downgrade_to_fork(
                &self.api_config.app_details.platform,
                &self.api_config.app_details.product,
            )
            .await?;

        Ok(forked_session)
    }

    /// Get the stash in use
    pub fn account_stash(&self) -> &Stash {
        &self.account_stash
    }

    /// Create a new instance of a use context.
    ///
    /// If the user context for a given user is still active, return
    /// the existing user context rather than creating a new one.
    ///
    /// If we detect that an existing context is active for a user
    /// but the session ids do not match we return an error.
    ///
    /// # Error
    ///
    /// Returns error if the user context failed to initialize or
    /// if we detect that we are trying to create duplicate contexts with
    /// different session id.
    pub async fn new_user_context(
        &self,
        user_id: UserId,
        session: ApiSession,
        session_id: SessionId,
    ) -> Result<Arc<UserContext>, CoreContextError> {
        let mut active_contexts = self.active_user_contexts.lock().await;

        // clean up any context that may have been dropped.
        active_contexts.retain(|_, value| value.strong_count() != 0);

        if let Some(context) = active_contexts.get(&user_id)
            && let Some(upgraded) = context.upgrade()
        {
            // If we are attempting to maintain uniqueness we can't
            // return the same context with different sessions
            // as this is not compatible.
            if session_id != *upgraded.session_id() {
                return Err(CoreContextError::DuplicateContext(user_id));
            }

            return Ok(upgraded);
        }

        // context is not register or it is no longer active.
        let db_path = self.user_db_path(&user_id);

        let cache_path = self.cache_path.join(user_id.as_str());
        let Some(context) = self.this.upgrade() else {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Failed to convert weak context to arc via upgrade"
            )));
        };
        let user_context = UserContext::new(
            session,
            context,
            &db_path,
            &self.user_db_initializers,
            user_id.clone(),
            session_id,
            cache_path,
        )
        .await?;

        active_contexts.insert(user_id, Arc::downgrade(&user_context));

        Ok(user_context)
    }

    /// Cancel all tasks associated with a user.
    pub async fn cancel_user_tasks(&self, user_id: &UserId) {
        if let Some(ctx) = (self.active_user_contexts.lock().await)
            .get(user_id)
            .and_then(Weak::upgrade)
        {
            ctx.cancel_all_tasks();
        }
    }

    pub fn log_service(&self) -> &LogService {
        self.get_service::<LoggingService>().service()
    }

    pub fn network_monitor_service(&self) -> &NetworkMonitorService {
        self.get_service::<NetworkMonitorService>()
    }

    /// Spawns a new task.
    ///
    /// Spawned task is bound to this context, i.e. it will get cancelled if
    /// this context gets cancelled as well.
    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        let token = self.cancellation_token.clone();

        self.task_service.spawn_cancellable(token, task)
    }

    /// Returns a cancellation token that is a child of the the one owned by the context.
    pub fn new_child_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    /// Cancel all tasks which are bound to this context.
    ///
    /// This will also cancel all child token created with [`child_cancellation_token()`]
    pub fn cancel_all_tasks(&self) {
        self.cancellation_token.cancel();
    }

    pub fn task_service(&self) -> &BackgroundAwareTaskService {
        &self.task_service
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation_token
    }

    pub fn clock(&self) -> &CoreClock {
        self.get_service::<CoreClock>()
    }

    #[allow(clippy::result_large_err)]
    pub fn session_observer_service(&self) -> &SessionObserverService {
        self.get_service::<SessionObserverService>()
    }

    #[allow(clippy::result_large_err)]
    pub fn event_service(&self) -> &ContextEventService {
        self.get_service::<ContextEventService>()
    }

    /// Retrieves the passphrase for the current session by decrypting the session's key secret.
    pub async fn get_session_passphrase(&self) -> Result<SecretVec<u8>, PassphraseAcquireError> {
        let session_id = self.get_primary_session_id().await?;
        let db_key = self.get_encryption_key()?;
        self.get_session(session_id)
            .await?
            .ok_or(PassphraseAcquireError::NoSession)?
            .key_secret
            .ok_or(PassphraseAcquireError::NoKeySecret)?
            .decrypt_to_bytes(&db_key)
            .map_err(|err| {
                error!("Failed to decrypt sessions key_secret: {err}");
                PassphraseAcquireError::KeySecretDecryption
            })
    }

    /// Retrieves the ID of the primary session for the primary account.
    pub async fn get_primary_session_id(&self) -> Result<SessionId, PassphraseAcquireError> {
        let primary_account = self
            .get_primary_account()
            .await?
            .ok_or(PassphraseAcquireError::NoPrimaryAccount)?;
        let session_id = self
            .get_account_sessions(primary_account.remote_id)
            .await?
            .first()
            .ok_or(PassphraseAcquireError::NoSessionId)?
            .remote_id
            .clone();
        Ok(session_id)
    }

    pub fn on_enter_foreground(&self) {
        self.event_service().publish(OnEnterForegroundEvent);
        self.task_service().resume_main();
    }

    pub async fn on_exit_foreground(&self) {
        self.event_service().publish(OnExitForegroundEvent);
        if let Err(e) = self
            .task_service()
            .pause_main_and_wait(Duration::from_millis(100))
            .await
        {
            warn!("Failed to await all paused work: {e:?}");
        }
    }

    pub fn on_exit_foreground_without_wait(&self) {
        self.event_service().publish(OnExitForegroundEvent);
        self.task_service().pause_main();
    }

    pub fn issue_reporter_service(&self) -> &IssueReporterService {
        self.get_service::<IssueReporterService>()
    }
}

impl Spawner for Context {
    fn spawn_task<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.spawn(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// We're running as the application.
    App,

    /// We're running as the share extension.
    ShareExt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Mobile,
}

impl Platform {
    #[must_use]
    #[allow(unreachable_code)]
    pub fn current() -> Self {
        #[cfg(target_os = "android")]
        return Self::Mobile;

        #[cfg(target_os = "ios")]
        return Self::Mobile;

        Self::Desktop
    }
}

#[derive(Error, Debug)]
pub enum PassphraseAcquireError {
    #[error("Error: {0}")]
    ContextError(#[from] CoreContextError),

    #[error("Could not find logged in primary account")]
    NoPrimaryAccount,

    #[error("Could not find session id")]
    NoSessionId,

    #[error("No key_secret for the session")]
    KeySecretDecryption,

    #[error("Could not find session")]
    NoSession,

    #[error("No key_secret for the session")]
    NoKeySecret,
}

fn get_account_db_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().join("account.db")
}

fn get_user_db_path(path: impl AsRef<Path>, user_id: &UserId) -> PathBuf {
    path.as_ref().join(user_id.to_string()).with_extension("db")
}

pub trait OnSessionDeleted: Send + 'static {
    /// Return true to be notified of further changes.
    fn on_session_deleted(
        &self,
        session_id: SessionId,
        user_id: UserId,
    ) -> impl Future<Output = OnSessionDeletedResponse> + Send;
}

/// Controls the behavior of future invocations to the session deleted observer.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OnSessionDeletedResponse {
    /// Keep this subscriber alive.
    Continue,
    /// Subscription no longer required.
    Terminate,
}

impl<H, Fut> OnSessionDeleted for H
where
    H: Fn(SessionId, UserId) -> Fut + Send + 'static,
    Fut: Future<Output = OnSessionDeletedResponse> + Send,
{
    fn on_session_deleted(
        &self,
        session_id: SessionId,
        user_id: UserId,
    ) -> impl Future<Output = OnSessionDeletedResponse> + Send {
        self(session_id, user_id)
    }
}

/// Implements the `InfoProvider` protocol from Muon. Used to pass the fingerprint to the Muon Client.
pub struct MuonInfoProvider {
    app_version: String,
    device_info_provider: DynDeviceInfoProvider,
}

#[async_trait]
impl InfoProvider for MuonInfoProvider {
    async fn fingerprint(&self) -> Option<Fingerprint> {
        let mut map = serde_json::Map::new();
        let key = format!("{}-challenge", self.app_version.replace('@', "-"));
        let value = json!(self.device_info_provider.get_device_info().await);
        map.insert(key, value);

        let result = serde_json::Value::Object(map);
        let fingerprint = result.into();

        Some(fingerprint)
    }
}
