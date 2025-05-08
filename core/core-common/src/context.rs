//! Core context contains all the necessary information to retrieve or create new accounts and sessions.

use crate::action_queue::CoreActionError;
use crate::auth_store::{AuthStore, DecryptExt};
use crate::datatypes::{
    LocalContactId, PasswordMode, StoredDevicePrivateKey, StoredDevicePublicKey, TfaStatus,
};
use crate::db::account::{
    CoreAccount, CoreSession, CoreSessionObserver, CoreSessionObserverNotification,
    SessionEncryptionKey,
};
use crate::db::migrations::migrate_account_db;
use crate::models::ModelExtension;
use crate::nuke_utils::{
    drop_all_tables_in_database, remove_or_clear_dir_safe, rename_database_files,
};
use crate::os::{KeyChain, KeyChainError, KeyChainExt, StoreInKeyChain};
use crate::pin_code::PinHash;
use crate::{KeyHandlingError, UserContext, UserDatabaseInitializer};
use anyhow::{Error as AnyhowError, anyhow};
use futures::TryFutureExt;
use itertools::Itertools;
use proton_action_queue::action::{Action, WriterGuardError};
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedError};
use proton_api_core::login::{Flow, LoginError};
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::BuildError;
use proton_api_core::services::proton::{SessionId, UserId};
use proton_api_core::session::Config as ApiConfig;
use proton_api_core::session::Session as ApiSession;
use proton_api_core::status_watcher::StatusWatcher;
use proton_api_core::verification::DynChallengeNotifier;
use proton_crypto_account::keys::PGPDeviceKey;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_sqlite3::MigratorError;
use proton_task_service::{AsyncTaskResult, DefaultTaskSpawner, TaskSpawner};
use proton_task_service::{BackgroundAwareTaskService, TaskService};
use proton_vcard::VcardValidationError;
use secrecy::ExposeSecret;
use stash::stash::{Stash, StashConfiguration, StashError, WatcherHandle};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use thiserror::Error;
use tokio::sync::{Mutex, broadcast};
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{Level, error, info, warn};

#[derive(Debug, Error)]
pub enum CoreContextError {
    #[error("Account with user id {0} is missing in the DB")]
    AccountMissing(UserId),
    #[error("Settings for user with id {0} are missing in the DB")]
    SettingsMissing(UserId),
    #[error("Build error: {0}")]
    Build(#[from] BuildError),
    #[error("Login error: {0}")]
    Login(#[from] LoginError),
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

impl proton_action_queue::action::Error for CoreContextError {
    fn is_network_failure(&self) -> bool {
        if let Self::Api(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("ContactCard not found for email: {0}")]
    CardNotFound(String),
    #[error("RemoteId not present for ContactCard for email: {0}")]
    ContactCardRemoteIdNotPresent(String),
    #[error("Contact not found for email: {0}")]
    FullContactNotFound(String),
    #[error("Validation: {0}")]
    Validation(#[from] VcardValidationError),
    #[error("Contact {0} does not have remote id")]
    ContactDoesNotHaveRemoteId(LocalContactId),
}

/// Represents the state of an account.
#[derive(Debug)]
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

    /// The account has no active sessions.
    LoggedOut,
}

impl CoreAccountState {
    fn of(account: &CoreAccount, sessions: &[CoreSession]) -> Self {
        let mut sessions_by_state = (sessions.iter())
            .map(|session| (CoreSessionState::of(session), session.remote_id.clone()))
            .into_group_map();

        // Does the account have any fully authenticated sessions?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::Authenticated) {
            return CoreAccountState::LoggedIn(sessions);
        }

        // Does the account have any sessions that are awaiting a mailbox password?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::NeedKey) {
            if account.password_mode.is_some_and(PasswordMode::want_mbp) {
                return CoreAccountState::NeedMbp(sessions);
            }
        }

        // Does the account have any sessions that are awaiting a second factor?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::NeedTfa) {
            if account.second_factor_mode.is_some_and(TfaStatus::want_tfa) {
                return CoreAccountState::NeedTfa(sessions);
            }
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

/// Result for core operations.
pub type CoreContextResult<T> = Result<T, CoreContextError>;

/// Context for core operations.
///
/// Acronyms used in the fields:
/// - `db`: Database
/// - `hv`: Human Verification
#[allow(dead_code)]
pub struct Context {
    this: Weak<Self>,
    user_db_path: PathBuf,
    account_db_path: PathBuf,
    log_path: Option<PathBuf>,
    account_stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
    active_user_contexts: Mutex<HashMap<UserId, Weak<UserContext>>>,
    cache_path: PathBuf,
    api_config: ApiConfig,
    hv_notifier: Option<DynChallengeNotifier>,
    cancellation_token: CancellationToken,
    task_service: BackgroundAwareTaskService,
    on_session_deleted_broadcast: broadcast::Sender<(SessionId, UserId)>,
}

const SESSION_OBSERVER_BROADCAST_CAPACITY: usize = 8;

impl Context {
    /// Create a new context by specifying the `account_db_path` where the account database will be created,
    /// an `user_db_path` for user databases, a`key_chain` implementation and a list of `initializers`
    /// for the user database.
    ///
    /// # Params
    /// * `account_db_path`: Path where the account db will be written.
    /// * `user_db_path`: Path where each user db will be written.
    /// * `key_chain`: Implementation of a keychain store.
    /// * `initializers`: List of user database initializers that should be called.
    /// * `api_config`: Configuration for any constructed API sessions.
    /// * `hv_notifier`: Optional notifier to handle human verification challenges.
    /// * `cache_path`: Cache path for cached data.
    /// * `connection_pool_size`: Maximum size of DB connection pool for the account DB. If `None`, the default value is used.
    ///
    /// # Errors
    /// Returns an error if the context failed to initialize correctly.
    ///
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        account_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        initializers: impl IntoIterator<Item = Box<dyn UserDatabaseInitializer>>,
        api_config: ApiConfig,
        hv_notifier: Option<DynChallengeNotifier>,
        cache_path: impl Into<PathBuf>,
        connection_pool_size: Option<u32>,
        log_path: Option<PathBuf>,
    ) -> CoreContextResult<Arc<Self>> {
        let initializers = initializers.into_iter().collect::<Vec<_>>();
        let account_db_path = account_db_path.into();
        let user_db_path = user_db_path.into();
        std::fs::create_dir_all(&account_db_path)?;
        std::fs::create_dir_all(&user_db_path)?;
        let account_stash_path = get_account_db_path(&account_db_path);
        let stash_config = StashConfiguration {
            path: Some(&account_stash_path),
            pool_size: connection_pool_size,
            ..Default::default()
        };
        let account_stash = Stash::new(stash_config)?;
        migrate_account_db(&account_stash).await?;

        let task_service = TaskService::new()?;

        let (broadcast_sender, _) = broadcast::channel(SESSION_OBSERVER_BROADCAST_CAPACITY);

        let session_observer = CoreSessionObserver::new(account_stash.clone())
            .await
            .inspect_err(|e| tracing::error!("Failed to create session observer: {e:?}"))?;

        let sender = broadcast_sender.clone();

        task_service.spawn(async move {
            on_session_deletion(session_observer, sender).await;
        });

        Ok(Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            user_db_path,
            account_db_path,
            log_path,
            key_chain,
            account_stash,
            user_db_initializers: initializers,
            active_user_contexts: Mutex::new(HashMap::new()),
            cache_path: cache_path.into(),
            api_config,
            hv_notifier,
            cancellation_token: CancellationToken::new(),
            task_service: BackgroundAwareTaskService::new(task_service),
            on_session_deleted_broadcast: broadcast_sender,
        }))
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
        Ok(CoreSession::all(&tether).await?)
    }

    /// Get all authenticated Core User Contexts.
    ///
    /// The method is suppose to be able to grab all users data in one call.
    /// The purpose is that there are few features which relay on being able to access
    /// All logged in accounts such as `background execution` & `pin verifcation`.
    ///
    pub async fn get_all_logged_in_user_ctx(&self) -> CoreContextResult<Vec<Arc<UserContext>>> {
        let sessions = self.get_sessions().await?;
        let mut ctxs = Vec::with_capacity(sessions.len());

        for session in sessions {
            if let CoreSessionState::Authenticated = CoreSessionState::of(&session) {
                ctxs.push(self.user_context_from_session(&session, None).await?);
            } else {
                tracing::warn!("Found unauthenticated session");
            }
        }

        Ok(ctxs)
    }

    /// Get path of the core cache in filesystem.
    ///
    #[must_use]
    pub(crate) fn get_cache_location(&self) -> &Path {
        self.cache_path.as_path()
    }

    /// Get path of the user database parent directory location
    ///
    #[must_use]
    pub(crate) fn get_user_db_location(&self) -> &Path {
        self.user_db_path.as_path()
    }

    /// Get path of account's database parent directory location
    ///
    #[must_use]
    pub(crate) fn get_account_db_location(&self) -> &Path {
        self.account_db_path.as_path()
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
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
        let tether = self.account_stash().connection();
        for account in CoreAccount::by_primary_at(&tether).await? {
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
        let mut tether = self.account_stash().connection();
        let mut account = CoreAccount::find_by_id(user_id, &tether)
            .await?
            .ok_or(CoreContextError::Other(anyhow!("account not found")))?
            .with_primary_now();

        tether.tx(async |tx| account.save(tx).await).await?;

        Ok(())
    }

    /// Create a new login flow for a new user.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no encryption key in the keychain.
    pub async fn new_login_flow(&self) -> CoreContextResult<Flow> {
        // Ensure we have an encryption key
        let _ = self.get_encryption_key()?;

        // Create a new API session
        let session = self.new_api_session(None, None).await?;

        // Create a new login flow
        Ok(Flow::new(session))
    }

    /// Create a new login flow for an existing user.
    ///
    /// This can be used to resume a login flow that was interrupted.
    /// For instance, if the user has already entered their login credentials,
    /// but the flow was interrupted while waiting for a second factor,
    /// the flow can be resumed by calling this method with the user and session IDs.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no encryption key in the keychain
    /// or if no session with the given IDs is able to be resumed.
    pub async fn resume_login_flow(
        &self,
        user_id: UserId,
        session_id: SessionId,
    ) -> CoreContextResult<Flow> {
        let key = self.get_encryption_key()?;
        let tether = self.account_stash().connection();

        let Some(account) = CoreAccount::find_by_id(user_id.clone(), &tether).await? else {
            return Err(CoreContextError::Other(anyhow!("account not found")));
        };

        let Some(session) = CoreSession::find_by_id(session_id.clone(), &tether).await? else {
            return Err(CoreContextError::Other(anyhow!("session not found")));
        };

        let password = (account.password)
            .map(|p| p.decrypt_to_string(&key))
            .transpose()
            .or(Err(CoreContextError::Crypto))?
            .map(|p| p.expose_secret().to_owned());

        match CoreSessionState::of(&session) {
            CoreSessionState::NeedTfa => Ok(Flow::new_from_tfa(
                self.new_api_session(Some(&session), None).await?,
                user_id,
                session_id,
                password,
            )),

            CoreSessionState::NeedKey => Ok(Flow::new_from_mbp(
                self.new_api_session(Some(&session), None).await?,
                user_id,
                session_id,
            )),

            CoreSessionState::Authenticated => Err(CoreContextError::Other(anyhow!(
                "session is already logged in"
            ))),
        }
    }

    /// Create a user context from a login flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow is not in the logged in state or if the user
    /// context could not be created.
    #[tracing::instrument(level=Level::DEBUG, skip_all)]
    pub async fn user_context_from_login_flow(
        &self,
        flow: &mut Flow,
    ) -> CoreContextResult<Arc<UserContext>> {
        if !flow.is_logged_in() {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        }

        let user_id: UserId = flow.user_id()?.to_owned();
        let session_id: SessionId = flow.session_id()?.to_owned();
        let session = flow.take_session()?;

        self.new_user_context(user_id, session, session_id).await
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
        status: Option<StatusWatcher>,
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
            .new_api_session(Some(session), status)
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
                .new_api_session(Some(&session), None)
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

        for session in self.get_account_sessions(user_id.clone()).await? {
            warn!("Orphan session found in database: {}", session.remote_id);

            let mut tether = self.account_stash().connection();
            tether.tx(async |tx| session.delete(tx).await).await?;
        }

        info!("logged out all sessions for account {user_id}");

        Ok(())
    }

    /// Removes an account, deleting all associated sessions and data.
    ///
    /// # Errors
    ///
    /// Returns an error if data can not be removed or the db operation failed.
    pub async fn delete_account(&self, user_id: UserId) -> CoreContextResult<()> {
        self.cancel_user_tasks(&user_id).await;
        self.delete_user_db(&user_id);

        // TODO(ET-231): User cache paths.

        let mut tether = self.account_stash().connection();
        tether
            .tx(async |tx| {
                CoreAccount::delete_by_id(user_id, tx)
                    .inspect_err(|e| error!("Failed to delete account from db: {e:?}"))
                    .await
            })
            .await?;

        Ok(())
    }

    /// Removes all data associated with the context, that includes:
    /// * Account database - drop all data, remove files
    /// * Core cache - all files under the cache path
    /// * Keychain - all secrets
    ///
    /// # Errors
    ///
    /// Returns an error if data can not be removed or the db operation failed.
    ///
    pub async fn tear_down(&self) {
        tracing::warn!("Remove all accounts data");
        let tether = self.account_stash().connection();
        let _ = drop_all_tables_in_database(tether).await.inspect_err(|e| {
            tracing::error!(
                "Could not drop database tables: `{e}`, will try to remove files anyway"
            );
        });
        tracing::warn!("Archive & remove account database");
        let account_db_location = self.get_account_db_location();
        rename_database_files(account_db_location).await;
        remove_or_clear_dir_safe(account_db_location).await;
        tracing::warn!("Clear cache");
        remove_or_clear_dir_safe(self.get_cache_location()).await;

        let _ = self
            .delete_secret::<SessionEncryptionKey>()
            .inspect_err(|e| tracing::error!("Could not remove session key: `{e}`"));
        let _ = self
            .delete_secret::<PinHash>()
            .inspect_err(|e| tracing::error!("Could not remove pin hash: `{e}`"));
        let _ = self
            .delete_secret::<StoredDevicePrivateKey>()
            .inspect_err(|e| tracing::error!("Could not remove device key: `{e}`"));
    }

    #[tracing::instrument(err, skip(self))]
    fn get_encryption_key(&self) -> CoreContextResult<SessionEncryptionKey> {
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
    pub fn gen_device_key_pair<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> CoreContextResult<StoredDevicePublicKey> {
        let key = PGPDeviceKey::generate(pgp_provider).map_err(|_| CoreContextError::Crypto)?;
        let private_key = key
            .serialize_to_secure_storage(pgp_provider)
            .map_err(|_| CoreContextError::Crypto)?;
        let private_key = StoredDevicePrivateKey::with_bytes(private_key.as_bytes().to_vec());
        let public_key = key
            .export_public_key(pgp_provider)
            .map_err(|_| CoreContextError::Crypto)?;

        self.key_chain
            .store::<StoredDevicePrivateKey>(private_key)?;

        Ok(StoredDevicePublicKey::from(public_key))
    }

    /// Interact with `KeyChain` to store a secret
    ///
    pub fn store_secret<S: StoreInKeyChain>(&self, secret: S) -> Result<(), KeyChainError> {
        self.key_chain.store::<S>(secret)
    }

    /// Interact with `KeyChain` to load a secret
    ///
    pub fn load_secret<S: StoreInKeyChain>(&self) -> Result<Option<S>, KeyChainError> {
        self.key_chain.load::<S>()
    }

    /// Interact with `KeyChain` to delete a secret
    ///
    pub fn delete_secret<S: StoreInKeyChain>(&self) -> Result<(), KeyChainError> {
        self.key_chain.delete::<S>()
    }

    fn user_db_path(&self, user_id: &UserId) -> PathBuf {
        get_user_db_path(&self.user_db_path, user_id)
    }

    /// Initializes a new API session, optionally pre-configured to use a specific core session.
    async fn new_api_session(
        &self,
        session: Option<&CoreSession>,
        status: Option<StatusWatcher>,
    ) -> CoreContextResult<ApiSession> {
        let user_id = session.map(|s| &s.account_id).cloned();
        let session_id = session.map(|s| &s.remote_id).cloned();
        let account_stash = self.account_stash();
        let keychain = Arc::clone(&self.key_chain);
        let store = AuthStore::new(account_stash, keychain, user_id, session_id);

        let mut builder = ApiSession::builder()
            .with_config(&self.api_config)
            .with_store(store);

        if let Some(status) = status {
            builder = builder.with_status(status);
        }

        if let Some(notifier) = &self.hv_notifier {
            builder = builder.with_notifier(Arc::clone(notifier));
        }

        Ok(builder.build().await?)
    }

    /// Get the stash in use
    pub fn account_stash(&self) -> &Stash {
        &self.account_stash
    }

    /// Delete the user's database files.
    ///
    /// This method just makes a best-effort attempt to delete the files it can find.
    /// Any errors are logged but not returned.
    fn delete_user_db(&self, user_id: &UserId) {
        let db = get_user_db_path(&self.user_db_path, user_id);
        let shm = db.with_extension("db-shm");
        let wal = db.with_extension("db-wal");

        for path in [db, shm, wal] {
            let Ok(true) = path.try_exists() else {
                continue;
            };

            if let Err(err) = std::fs::remove_file(&path) {
                error!(?err, "failed to erase user database file");
            }
        }
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
    async fn new_user_context(
        &self,
        user_id: UserId,
        session: ApiSession,
        session_id: SessionId,
    ) -> Result<Arc<UserContext>, CoreContextError> {
        let mut active_contexts = self.active_user_contexts.lock().await;

        // clean up any context that may have been dropped.
        active_contexts.retain(|_, value| value.strong_count() != 0);

        if let Some(context) = active_contexts.get(&user_id) {
            if let Some(upgraded) = context.upgrade() {
                // If we are attempting to maintain uniqueness we can't
                // return the same context with different sessions
                // as this is not compatible.
                if session_id != *upgraded.session_id() {
                    return Err(CoreContextError::DuplicateContext(user_id));
                }

                return Ok(upgraded);
            }
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

    pub fn get_log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    /// Spawns a new task.
    ///
    /// Spawned task is bound to this context, i.e. it will get cancelled if
    /// this context gets cancelled as well.
    pub fn spawn<F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.spawn_with::<DefaultTaskSpawner, _>(task)
    }

    /// Like [`Self::spawn()`], but using given [`TaskSpawner`].
    pub fn spawn_with<S, F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        S: TaskSpawner,
        F: Future<Output: Send> + Send + 'static,
    {
        let token = self.cancellation_token.clone();

        self.task_service
            .spawn_cancellable_with::<S, _>(token, task)
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

    /// Subscribes for the event of closing the session. Use it to cleanup any remaining tasks
    /// or memory footprints.
    ///
    pub fn on_session_deleted(&self, hook: impl OnSessionDeleted) {
        let mut receiver = self.on_session_deleted_broadcast.subscribe();
        self.task_service.spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok((session_id, user_id)) => {
                        if hook.on_session_deleted(session_id, user_id).await
                            == OnSessionDeletedResponse::Terminate
                        {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => {
                        return;
                    }
                }
            }
        });
    }
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
#[tracing::instrument(skip_all)]
async fn on_session_deletion(
    mut observer: CoreSessionObserver,
    hook_sender: broadcast::Sender<(SessionId, UserId)>,
) {
    tracing::debug!("Starting task");
    while let Ok(notifications) = observer.next().await {
        tracing::debug!("Task received: {:?}", notifications);
        for notification in notifications {
            if let CoreSessionObserverNotification::Deleted(session_id, user_id) = notification {
                tracing::debug!("User {user_id}'s session {session_id} has been deleted");
                _ = hook_sender.send((session_id, user_id));
            }
        }
    }
    tracing::debug!("Stopping task");
}
