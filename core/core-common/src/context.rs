//! Core context contains all the necessary information to retrieve or create new accounts and sessions.
use crate::auth_store::{AuthStore, DecryptExt};
use crate::cache::CacheError;
use crate::datatypes::{LocalId, RemoteId};
use crate::db::account::{CoreAccount, CoreSession, SessionEncryptionKey};
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::db::ChangeReceiver;
use crate::models::ModelExtension;
use crate::os::{KeyChain, KeyChainError};
use crate::{KeyHandlingError, UserContext, UserDatabaseInitializer};
use anyhow::{anyhow, Error as AnyhowError};
use futures::TryFutureExt;
use itertools::Itertools;
use proton_api_core::login::{Flow, LoginError};
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Config as ApiConfig;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::Session as ApiCoreSession;
use proton_sqlite3::MigratorError;
use proton_vcard::VcardValidationError;
use secrecy::{ExposeSecret, SecretString};
use stash::stash::{Stash, StashError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use thiserror::Error;
use tokio::task::JoinError;
use tracing::{debug, error, info, Level};

#[derive(Debug, Error)]
pub enum CoreContextError {
    #[error("Login error: {0}")]
    Login(#[from] LoginError),
    #[error("API error: {0}")]
    Api(#[from] ApiServiceError),
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
    #[error("RemoteId not present for local_id: {0}")]
    MissingRemoteId(LocalId),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(#[from] KeyHandlingError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Cache error: {0}")]
    CacheError(#[from] CacheError),
    #[error("Problem with loading contact: {0}")]
    ContactError(#[from] ContactError),
    #[error("{0}")]
    Other(AnyhowError),
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
}

/// Represents the state of an account.
#[derive(Debug)]
pub enum CoreAccountState {
    /// The account is not yet ready to be used.
    NotReady,

    /// The account has at least one fully logged-in session;
    /// the variant holds the (remote) IDs of the fullly logged-in sessions.
    LoggedIn(Vec<RemoteId>),

    /// The account has authenticated sessions but they are missing the key secret.
    /// The variant holds the (remote) IDs of the sessions that are missing the key secret.
    NeedMbp(Vec<RemoteId>),

    /// The account has partially authenticated sessions that require a second factor.
    /// The variant holds the (remote) IDs of the sessions that require a second factor.
    NeedTfa(Vec<RemoteId>),

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
            if account.password_mode.want_mbp() {
                return CoreAccountState::NeedMbp(sessions);
            }
        }

        // Does the account have any sessions that are awaiting a second factor?
        if let Some(sessions) = sessions_by_state.remove(&CoreSessionState::NeedTfa) {
            if account.second_factor_mode.want_tfa() {
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
    fn of(session: &CoreSession) -> Self {
        if session.auth_state.need_tfa() {
            CoreSessionState::NeedTfa
        } else if session.key_secret.is_none() {
            CoreSessionState::NeedKey
        } else {
            CoreSessionState::Authenticated
        }
    }
}

/// Callback when the status of the network changes.
pub trait NetworkStatusChanged: Send + Sync {
    fn on_network_status_changed(&self, online: bool);
}

/// Result for core operations.
pub type CoreContextResult<T> = Result<T, CoreContextError>;

/// Context for core operations.
#[allow(dead_code)]
pub struct Context {
    this: Weak<Self>,
    network_connected: AtomicBool,
    user_db_path: PathBuf,
    stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
    api: Proton,
    network_callback: Option<Box<dyn NetworkStatusChanged>>,
}

impl Context {
    /// Create a new context by specifying the `account_db_path` where the account database will be created,
    /// an `user_db_path` for user databases, a`key_chain` implementation and a list of `initializers`
    /// for the user database.
    ///
    /// # Params
    /// * `async_runtime`: Instance of a multithreaded async runtime.
    /// * `account_db_path`: Path where the account db will be written.
    /// * `user_db_path`: Path where each user db will be written.
    /// * `key_chain`: Implementation of a keychain store.
    /// * `initializers`: List of user database initializers that should be called.
    /// * `client`: Instance of the http client.
    /// * `network_callback`: Callback to be notified of network status changes.
    ///
    /// # Errors
    /// Returns an error if the context failed to initialize correctly.
    ///
    pub async fn new(
        account_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        initializers: impl IntoIterator<Item = Box<dyn UserDatabaseInitializer>>,
        api_config: ApiConfig,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> CoreContextResult<Arc<Self>> {
        let initializers = initializers.into_iter().collect::<Vec<_>>();
        let account_db_path = account_db_path.into();
        let user_db_path = user_db_path.into();
        std::fs::create_dir_all(&account_db_path)?;
        std::fs::create_dir_all(&user_db_path)?;
        let account_db_path = get_account_db_path(account_db_path);
        let stash = Stash::new(Some(&account_db_path))?;
        migrate_account_db(&stash).await?;

        let api = Proton::new(api_config, None, None)
            .await
            .map_err(ApiServiceError::from)?;

        Ok(Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            network_connected: AtomicBool::new(true),
            user_db_path,
            key_chain,
            stash,
            user_db_initializers: initializers,
            network_callback,
            api,
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
        Ok(CoreAccount::all(&self.stash, None).await?)
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
    pub async fn watch_accounts(
        &self,
    ) -> CoreContextResult<(Vec<CoreAccount>, ChangeReceiver<CoreAccount>)> {
        let (tx, rx) = flume::unbounded();

        let res = CoreAccount::all(&self.stash, Some(tx)).await?;

        Ok((res, rx))
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
        Ok(CoreSession::all(&self.stash, None).await?)
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
    pub async fn watch_sessions(
        &self,
    ) -> CoreContextResult<(Vec<CoreSession>, ChangeReceiver<CoreSession>)> {
        let (tx, rx) = flume::unbounded();

        let res = CoreSession::all(&self.stash, Some(tx)).await?;

        Ok((res, rx))
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
        user_id: RemoteId,
    ) -> CoreContextResult<Vec<CoreSession>> {
        Ok(CoreSession::find_by_user_id(user_id, &self.stash, None).await?)
    }

    /// Watch an account's API sessions for changes.
    ///
    /// See [`Context::watch_sessions`] for more information on watching API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher cannot be registered with the database.
    pub async fn watch_account_sessions(
        &self,
        user_id: RemoteId,
    ) -> CoreContextResult<(Vec<CoreSession>, ChangeReceiver<CoreSession>)> {
        let (tx, rx) = flume::unbounded();

        let res = CoreSession::find_by_user_id(user_id, &self.stash, Some(tx)).await?;

        Ok((res, rx))
    }

    /// Get a single account by its remote (user) ID.
    ///
    /// This is a convenience method that enables retrieving a single account without requiring
    /// the full set of accounts to be loaded first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account(&self, user_id: RemoteId) -> CoreContextResult<Option<CoreAccount>> {
        Ok(CoreAccount::find_by_id(user_id, &self.stash).await?)
    }

    /// Get the login state of an account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account_state(
        &self,
        user_id: RemoteId,
    ) -> CoreContextResult<Option<CoreAccountState>> {
        let Some(account) = CoreAccount::find_by_id(user_id.clone(), &self.stash).await? else {
            return Ok(None);
        };

        let state = CoreSession::find_by_user_id(user_id, &self.stash, None)
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
        session_id: RemoteId,
    ) -> CoreContextResult<Option<CoreSession>> {
        Ok(CoreSession::find_by_id(session_id, &self.stash).await?)
    }

    /// Get the login state of a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session_state(
        &self,
        session_id: RemoteId,
    ) -> CoreContextResult<Option<CoreSessionState>> {
        let Some(session) = CoreSession::find_by_id(session_id, &self.stash).await? else {
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
        for account in CoreAccount::by_primary_at(&self.stash).await? {
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
    pub async fn set_primary_account(&self, user_id: RemoteId) -> CoreContextResult<()> {
        CoreAccount::find_by_id(user_id, &self.stash)
            .await?
            .ok_or(CoreContextError::Other(anyhow!("account not found")))?
            .with_primary_now()
            .save()
            .await?;

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
        let session = self.new_api_session(None).await?;

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
        user_id: RemoteId,
        session_id: RemoteId,
    ) -> CoreContextResult<Flow> {
        let Some(account) = CoreAccount::find_by_id(user_id.clone(), &self.stash).await? else {
            return Err(CoreContextError::Other(anyhow!("account not found")));
        };

        let Some(session) = CoreSession::find_by_id(session_id.clone(), &self.stash).await? else {
            return Err(CoreContextError::Other(anyhow!("session not found")));
        };

        match CoreSessionState::of(&session) {
            CoreSessionState::NeedTfa => Ok(Flow::resume_second_factor(
                self.new_api_session(Some(&session)).await?,
                user_id.into(),
                session_id.into(),
                account.second_factor_mode.into(),
            )),

            CoreSessionState::NeedKey => Ok(Flow::resume_mailbox_password(
                self.new_api_session(Some(&session)).await?,
                user_id.into(),
                session_id.into(),
                account.password_mode.into(),
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
    #[tracing::instrument(level=Level::DEBUG, skip(self, flow, cache_path, cache_size))]
    pub async fn user_context_from_login_flow(
        &self,
        flow: &Flow,
        cache_path: PathBuf,
        cache_size: u32,
    ) -> CoreContextResult<UserContext> {
        if !flow.is_logged_in() {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        }

        let user_id: RemoteId = flow.user_id()?.to_owned().into();
        let session_id: RemoteId = flow.session_id()?.to_owned().into();
        let session = flow.session().to_owned();
        let stash = self.new_user_db_pool(&user_id).await?;

        UserContext::new(session, stash, user_id, session_id, cache_path, cache_size).await
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
        cache_path: PathBuf,
        cache_size: u32,
    ) -> CoreContextResult<UserContext> {
        // Ensure we have an encryption key
        let key = self.get_encryption_key()?;

        // Ensure the key can be used to decrypt the access token
        let _ = session
            .access_token
            .decrypt_to_string(&key)
            .or(Err(CoreContextError::Crypto))?;

        // Ensure the key can be used to decrypt the refresh token
        let _ = session
            .refresh_token
            .decrypt_to_string(&key)
            .or(Err(CoreContextError::Crypto))?;

        let user_id = session.account_id.clone();
        let session_id = session.remote_id.clone();
        let session = self.new_api_session(Some(session)).await?;
        let stash = self.new_user_db_pool(&user_id).await?;

        UserContext::new(session, stash, user_id, session_id, cache_path, cache_size).await
    }

    /// Logs out all sessions of an account without deleting the account's data.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn logout_account(&self, user_id: RemoteId) -> CoreContextResult<()> {
        for session in &self.get_account_sessions(user_id).await? {
            let Ok(api) = self
                .new_api_session(Some(session))
                .inspect_err(|err| error!("failed to create API session: {err}"))
                .await
            else {
                continue;
            };

            let Ok(()) = api
                .logout()
                .inspect_err(|err| error!("failed to logout API session: {err}"))
                .await
            else {
                continue;
            };

            info!("logged out session {}", session.remote_id);
        }

        Ok(())
    }

    /// Removes an account, deleting all associated sessions and data.
    ///
    /// # Errors
    ///
    /// Returns an error if data can not be removed or the db operation failed.
    pub async fn delete_account(&self, user_id: RemoteId) -> CoreContextResult<()> {
        if let Some(path) = self.find_user_db(&user_id) {
            tokio::fs::remove_file(&path)
                .map_err(|e| CoreContextError::Other(anyhow!("Failed to erase user database: {e}")))
                .inspect_err(|e| error!("{e}"))
                .await?;
        }

        // TODO(ET-231): User cache paths.

        CoreAccount::delete_by_remote_id(user_id, &self.stash)
            .inspect_err(|e| error!("Failed to delete account from db: {e}"))
            .await?;

        Ok(())
    }

    pub fn set_network_connected(&self, value: bool) {
        let old_value = self.network_connected.load(Ordering::Acquire);
        if old_value != value {
            self.network_connected.store(value, Ordering::Release);
            if let Some(cb) = &self.network_callback {
                cb.on_network_status_changed(value);
            }
        }
    }

    /// Check whether a network connection is available.
    #[must_use]
    pub fn is_network_corrected(&self) -> bool {
        self.network_connected.load(Ordering::Relaxed)
    }

    fn get_encryption_key(&self) -> CoreContextResult<SessionEncryptionKey> {
        let Some(key) = self.key_chain.get().map_err(CoreContextError::KeyChain)? else {
            return Err(CoreContextError::KeyChainHasNoKey);
        };
        let key = SecretString::new(key);
        SessionEncryptionKey::from_base64(key.expose_secret()).ok_or(CoreContextError::Crypto)
    }

    async fn new_user_db_pool(&self, user_id: &RemoteId) -> Result<Stash, MigratorError> {
        let user_db_path = get_user_db_path(&self.user_db_path, user_id);
        let stash = Stash::new(Some(&user_db_path))?;
        debug!("initializing core database");
        // initialize core db
        migrate_account_db(&stash).await?;
        migrate_core_db(&stash).await?;
        debug!("initializing user ");
        // initialize user db
        for initializer in &self.user_db_initializers {
            initializer.initialize(&stash)?;
        }

        Ok(stash)
    }

    /// Initializes a new API session, optionally pre-configured to use a specific core session.
    async fn new_api_session(
        &self,
        session: Option<&CoreSession>,
    ) -> CoreContextResult<ApiCoreSession> {
        let user_id = session.map(|s| &s.account_id).cloned();
        let session_id = session.map(|s| &s.remote_id).cloned();
        let stash = self.stash.clone();
        let keychain = Arc::clone(&self.key_chain);
        let store = AuthStore::new(stash, keychain, user_id, session_id);
        let config = self.api.config().to_owned();

        Ok(ApiCoreSession::new(config, Some(Box::new(store)))
            .map_err(ApiServiceError::from)
            .await?)
    }

    /// Get the API service
    pub fn api(&self) -> &Proton {
        &self.api
    }

    /// Get the stash in use
    pub fn stash(&self) -> &Stash {
        &self.stash
    }

    /// Find the user's database file.
    fn find_user_db(&self, user_id: &RemoteId) -> Option<PathBuf> {
        let path = get_user_db_path(&self.user_db_path, user_id);

        if path.try_exists().is_ok() {
            Some(path)
        } else {
            None
        }
    }
}

fn get_account_db_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().join("account.db")
}

fn get_user_db_path(path: impl AsRef<Path>, user_id: &RemoteId) -> PathBuf {
    path.as_ref().join(user_id.to_string()).with_extension("db")
}
