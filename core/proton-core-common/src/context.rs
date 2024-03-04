//! Core context contains all the necessary information to retrieve or create new sessions.
use crate::os::{KeyChain, KeyChainError};
use crate::session::CoreSession;
use crate::user_context::{UserContext, UserDatabaseInitializer};
use crate::CoreSessionCallback;
use proton_api_core::auth::{new_arc_auth_store, ArcAuthStore};
use proton_api_core::domain::UserId;
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::proton_sqlite3::SqliteMode;
use proton_api_core::exports::tracing::debug;
use proton_api_core::exports::tracing::Level;
use proton_api_core::exports::{anyhow, thiserror, tracing};
use proton_api_core::login::LoginFlow;
use proton_api_core::{http, Session};
use proton_core_db::proton_sqlite3::SqliteConnectionPool;
use proton_core_db::{
    migrate_core_db, migrate_session_db, EncryptedUserSession, SessionEncryptionKey,
    SessionSqliteConnection,
};
use proton_event_loop::proton_async::runtime::MTRuntime;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum CoreContextError {
    #[error("Database Error: {0}")]
    DB(#[from] proton_core_db::DBError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] proton_core_db::DBMigrationError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("{0}")]
    Other(anyhow::Error),
}

/// Callback when the status of the network changes.
pub trait NetworkStatusChanged: Send + Sync {
    fn on_network_status_changed(&self, online: bool);
}

/// Result for core operations.
pub type CoreContextResult<T> = Result<T, CoreContextError>;

/// Context for core operations.
#[derive(Clone)]
pub struct CoreContext {
    inner: Arc<CoreContextInner>,
}

struct CoreContextInner {
    runtime: MTRuntime,
    network_connected: AtomicBool,
    user_db_path: PathBuf,
    session_db: SqliteConnectionPool,
    key_chain: Arc<dyn KeyChain>,
    user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
    network_callback: Option<Box<dyn NetworkStatusChanged>>,
}

impl CoreContext {
    /// Create a new context by specifying the `session_db_path` where the session database will be created,
    /// an `user_db_path` for user databases, a`key_chain` implementation and a list of `initializers`
    /// for the user database.
    pub fn new(
        async_runtime: MTRuntime,
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        key_chain: Arc<dyn KeyChain>,
        initializers: impl IntoIterator<Item = Box<dyn UserDatabaseInitializer>>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> CoreContextResult<Self> {
        let initializers = initializers.into_iter().collect::<Vec<_>>();
        let session_db_path = session_db_path.into();
        let user_db_path = user_db_path.into();
        Self::_new(
            async_runtime,
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            network_callback,
        )
    }
    fn _new(
        async_runtime: MTRuntime,
        session_db_path: PathBuf,
        user_db_path: PathBuf,
        key_chain: Arc<dyn KeyChain>,
        initializers: Vec<Box<dyn UserDatabaseInitializer>>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> CoreContextResult<Self> {
        // create path.
        std::fs::create_dir_all(&session_db_path)?;
        std::fs::create_dir_all(&user_db_path)?;
        let session_db_path = get_session_db_path(&session_db_path);

        let pool = SqliteConnectionPool::new(
            SqliteMode::File(session_db_path.clone()),
            db_debug_enabled(),
        );
        {
            let mut connection = pool.acquire()?;
            migrate_session_db(&mut connection)?;
        }

        Ok(Self {
            inner: Arc::new(CoreContextInner {
                runtime: async_runtime,
                network_connected: AtomicBool::new(true),
                user_db_path,
                key_chain,
                session_db: pool,
                user_db_initializers: initializers,
                network_callback,
            }),
        })
    }

    pub fn async_runtime(&self) -> &MTRuntime {
        &self.inner.runtime
    }

    /// Get available sessions.
    pub fn get_sessions(&self) -> CoreContextResult<Vec<EncryptedUserSession>> {
        let conn = self.get_connection()?;
        let r = conn.as_connection_ref().load_all_sessions()?;
        Ok(r)
    }

    /// Create a new login flow for a new user.
    pub fn new_login_flow(
        &self,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> CoreContextResult<LoginFlow> {
        // Check if we have an encryption key
        let _ = self.get_encryption_key()?;
        let core_session = new_arc_auth_store(CoreSession::new(
            None,
            self.inner.session_db.clone(),
            self.inner.key_chain.clone(),
            cb,
        ));

        let session = new_session(core_session)?;
        Ok(LoginFlow::new(session))
    }

    /// Create a user context from a login flow. This will fail if the flow is not in the
    /// logged in state.
    #[tracing::instrument(level=Level::DEBUG, skip(self, login_flow))]
    pub fn user_context_from_login_flow(
        &self,
        login_flow: &LoginFlow,
    ) -> CoreContextResult<UserContext> {
        if !login_flow.is_logged_in() {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        }

        let Some(user) = login_flow.user() else {
            return Err(CoreContextError::Other(anyhow!("invalid login state")));
        };

        debug!("Creating new context for user {}({})", user.email, user.id);
        let db = self.new_user_db_pool(&user.id)?;

        let ctx = UserContext::new(login_flow.session().clone(), db, user.id.clone())?;

        Ok(ctx)
    }

    /// Get a user context from an existing session.
    #[tracing::instrument(level=Level::DEBUG, skip(self,session, cb), fields(user_id=?session.user_id, uid=?session.session_id))]
    pub fn user_context_from_session(
        &self,
        session: &EncryptedUserSession,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> CoreContextResult<UserContext> {
        let db = self.new_user_db_pool(&session.user_id)?;
        debug!("decrypting session tokens");
        let key = self.get_encryption_key()?;
        let decrypted_session = session
            .to_decrypted_session(&key)
            .map_err(|_| CoreContextError::Crypto)?;
        let user_id = session.user_id.clone();
        let core_session = new_arc_auth_store(CoreSession::new(
            Some(decrypted_session),
            self.inner.session_db.clone(),
            self.inner.key_chain.clone(),
            cb,
        ));
        debug!("Creating session");
        let session = new_session(core_session)?;
        let ctx = UserContext::new(session, db, user_id)?;
        Ok(ctx)
    }

    pub fn set_network_connected(&self, value: bool) {
        let old_value = self.inner.network_connected.load(Ordering::Acquire);
        if old_value != value {
            self.inner.network_connected.store(value, Ordering::Release);
            if let Some(cb) = &self.inner.network_callback {
                cb.on_network_status_changed(value);
            }
        }
    }

    pub fn is_network_corrected(&self) -> bool {
        self.inner.network_connected.load(Ordering::Relaxed)
    }
    fn get_connection(&self) -> CoreContextResult<SessionSqliteConnection> {
        let conn = self.inner.session_db.acquire()?;
        Ok(conn.into())
    }

    fn get_encryption_key(&self) -> CoreContextResult<SessionEncryptionKey> {
        let Some(key) = self
            .inner
            .key_chain
            .get()
            .map_err(CoreContextError::KeyChain)?
        else {
            return Err(CoreContextError::KeyChainHasNoKey);
        };

        SessionEncryptionKey::with_bytes(key).map_err(|mut v| {
            v.fill(0);
            CoreContextError::Crypto
        })
    }

    fn new_user_db_pool(&self, user_id: &UserId) -> CoreContextResult<SqliteConnectionPool> {
        let user_db_path = get_user_db_path(&self.inner.user_db_path, user_id);
        let pool = SqliteConnectionPool::new(SqliteMode::File(user_db_path), db_debug_enabled());
        let mut conn = pool.acquire()?;
        debug!("initializing core database");
        // initialize core db
        {
            migrate_core_db(&mut conn)?;
        }
        debug!("initializing user ");
        // initialize user db
        for initializer in &self.inner.user_db_initializers {
            initializer.initialize(&mut conn)?;
        }

        Ok(pool)
    }
}

fn get_session_db_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().join("session.db")
}

fn get_user_db_path(path: impl AsRef<Path>, user_id: &UserId) -> PathBuf {
    path.as_ref().join(user_id.to_string()).with_extension("db")
}

fn new_session(arc_auth_store: ArcAuthStore) -> CoreContextResult<Session> {
    let client = http::ClientBuilder::new()
        .app_version("Other")
        .build()
        .map_err(|e| CoreContextError::Other(anyhow!("Failed to create client: {e}")))?;
    Ok(Session::new(client, arc_auth_store))
}

fn db_debug_enabled() -> bool {
    std::env::var("PROTON_CORE_CTX_DB_DEBUG").is_ok()
}
