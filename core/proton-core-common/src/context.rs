//! Core context contains all the necessary information to retrieve or create new sessions.
use crate::auth_store::AuthStore;
use crate::cache::CacheError;
use crate::datatypes::RemoteId;
use crate::db::migrations::{migrate_core_db, migrate_session_db};
use crate::db::session::{EncryptedUserSession, SessionEncryptionKey};
use crate::os::{KeyChain, KeyChainError};
use crate::session::Session;
use crate::{KeyHandlingError, UserContext, UserDatabaseInitializer};
use anyhow::{anyhow, Error as AnyhowError};
use proton_api_core::login::Flow;
use proton_api_core::service::{ApiService, ApiServiceError};
use proton_api_core::services::proton::Config as ApiConfig;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::Session as ApiCoreSession;
use proton_sqlite3::MigratorError;
use secrecy::{ExposeSecret, SecretString};
use stash::orm::Model;
use stash::params;
use stash::stash::{Interface, Stash, StashError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use thiserror::Error;
use tracing::{debug, error, Level};
use url::Url;

#[derive(Debug, Error)]
pub enum CoreContextError {
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

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("ContactCard not found for email: {0}")]
    CardNotFound(String),
    #[error("RemoteId not present for ContactCard for email: {0}")]
    ContactCardRemoteIdNotPresent(String),
    #[error("Contact not found for email: {0}")]
    FullContactNotFound(String),
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
    session_stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
    api: Proton,
    network_callback: Option<Box<dyn NetworkStatusChanged>>,
}

impl Context {
    /// Create a new context by specifying the `session_db_path` where the session database will be created,
    /// an `user_db_path` for user databases, a`key_chain` implementation and a list of `initializers`
    /// for the user database.
    ///
    /// # Params
    /// * `async_runtime`: Instance of a multithreaded async runtime.
    /// * `session_db_path`: Path where the session db will be written.
    /// * `user_db_path`: Path where each user db will be written.
    /// * `key_chain`: Implementation of a keychain store.
    /// * `initializers`: List of user database initializers that should be called.
    /// * `client`: Instance of the http client.
    /// * `network_callback`: Callback to be notified of network status changes.
    ///
    /// # Errors
    /// Returns error if the context failed to initialize correctly.
    ///
    pub async fn new(
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        initializers: impl IntoIterator<Item = Box<dyn UserDatabaseInitializer>>,
        api_url: Url,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> CoreContextResult<Arc<Self>> {
        let initializers = initializers.into_iter().collect::<Vec<_>>();
        let session_db_path = session_db_path.into();
        let user_db_path = user_db_path.into();
        std::fs::create_dir_all(&session_db_path)?;
        std::fs::create_dir_all(&user_db_path)?;
        let session_db_path = get_session_db_path(session_db_path);
        let stash = Stash::new(Some(&session_db_path))?;
        migrate_session_db(&stash).await?;

        let api = Proton::new(
            ApiConfig {
                base_url: api_url.to_string(),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .map_err(ApiServiceError::from)?;

        Ok(Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            network_connected: AtomicBool::new(true),
            user_db_path,
            key_chain,
            session_stash: stash,
            user_db_initializers: initializers,
            network_callback,
            api,
        }))
    }

    /// Get available sessions.
    ///
    /// # Errors
    /// Returns error if we fail to retrieve the sessions from the db.
    pub async fn get_sessions(&self) -> Result<Vec<EncryptedUserSession>, StashError> {
        EncryptedUserSession::find(String::new(), vec![], &self.session_stash, None).await
    }

    /// Create a new login flow for a new user.
    ///
    /// # Errors
    ///
    /// Returns error if there is no encryption key in the keychain.
    pub async fn new_login_flow(&self) -> CoreContextResult<Flow> {
        // Check if we have an encryption key
        let _ = self.get_encryption_key()?;
        let _core_session = Session::new(None, self.session_stash.clone(), self.key_chain.clone());

        let auth_store = AuthStore::new(
            self.session_stash.clone(),
            Arc::clone(&self.key_chain),
            None,
        );

        let session = ApiCoreSession::new(
            ApiConfig {
                base_url: self.api.base_url().to_string(),
                ..Default::default()
            },
            Some(Box::new(auth_store)),
        )
        .await
        .map_err(ApiServiceError::from)?;
        Ok(Flow::new(session))
    }

    /// Create a user context from a login flow. This will fail if the flow is not in the
    /// logged in state.
    #[tracing::instrument(level=Level::DEBUG, skip(self, login_flow, cache_path, cache_size))]
    pub async fn user_context_from_login_flow(
        &self,
        login_flow: &Flow,
        cache_path: impl Into<PathBuf>,
        cache_size: u32,
    ) -> CoreContextResult<UserContext> {
        if !login_flow.is_logged_in() {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        }

        let Some(user) = login_flow.user() else {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        };

        debug!("Creating new context for user {}({})", user.email, user.id);
        let stash = self.new_user_db_pool(&user.id.clone().into()).await?;

        UserContext::new(
            login_flow.session().clone(),
            stash,
            user.id.clone().into(),
            cache_path.into(),
            cache_size,
        )
    }

    /// Get a user context from an existing session.
    ///
    /// # Errors
    ///
    /// TODO: Document errors
    ///
    pub async fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
        cache_path: impl Into<PathBuf>,
        cache_size: u32,
    ) -> CoreContextResult<UserContext> {
        let stash = self.new_user_db_pool(&session.user_id).await?;
        debug!("decrypting session tokens");
        let key = self.get_encryption_key()?;
        let decrypted_session = session
            .to_decrypted_session(&key)
            .map_err(|_| CoreContextError::Crypto)?;
        let user_id = session.user_id.clone();
        let _core_session = Session::new(
            Some(decrypted_session),
            self.session_stash.clone(),
            self.key_chain.clone(),
        );

        let auth_store = AuthStore::new(
            self.session_stash.clone(),
            Arc::clone(&self.key_chain),
            Some(user_id.clone()),
        );

        debug!("Creating session");
        let session = ApiCoreSession::new(
            ApiConfig {
                base_url: self.api.base_url().to_string(),
                ..Default::default()
            },
            Some(Box::new(auth_store)),
        )
        .await
        .map_err(ApiServiceError::from)?;
        UserContext::new(session, stash, user_id, cache_path.into(), cache_size)
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

    /// Removes a user session and deletes all associated data.
    ///
    /// # Errors
    /// Returns error if data can not be removed or the db operation failed.
    pub async fn delete_session(&self, session: &EncryptedUserSession) -> CoreContextResult<()> {
        let db_path = get_user_db_path(&self.user_db_path, &session.user_id);
        std::fs::remove_file(db_path).map_err(|e| {
            let e = anyhow!("Failed to erase user database: {e}");
            error!("{e}");
            CoreContextError::Other(e)
        })?;

        //TODO(ET-231): User cache paths.

        self.session_stash
            .execute(
                "DELETE FROM core_sessions WHERE user_id =?",
                params![session.user_id.clone()],
            )
            .await
            .map_err(|e| {
                error!("Failed to delete session from db: {e}");
                e
            })?;

        Ok(())
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
        migrate_core_db(&stash).await?;
        debug!("initializing user ");
        // initialize user db
        for initializer in &self.user_db_initializers {
            initializer.initialize(&stash)?;
        }

        Ok(stash)
    }

    /// Get the API service
    pub fn api(&self) -> &Proton {
        &self.api
    }

    /// Get the stash in use
    pub fn stash(&self) -> &Stash {
        &self.session_stash
    }
}

fn get_session_db_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().join("session.db")
}

fn get_user_db_path(path: impl AsRef<Path>, user_id: &RemoteId) -> PathBuf {
    path.as_ref().join(user_id.to_string()).with_extension("db")
}
