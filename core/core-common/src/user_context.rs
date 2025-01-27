pub use self::keys::*;
use crate::cache::ProtonCache;
use crate::datatypes::{AccountDetails, ConnectionStatus};
use crate::db::account::CoreAccount;
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::models::sender_image_cache::SenderImage;
use crate::{Context, CoreContextError, CoreContextResult};
use proton_api_core::services::proton::common::{AuthId, UserId};
use proton_api_core::services::proton::{ProtonCore, ONE_SECOND_TIMEOUT};
use proton_api_core::session::{CoreSession, Session};
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::Stash;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

pub mod images_logo;
mod keys;

/// Extra initializer for the user database.
pub trait UserDatabaseInitializer: Send + Sync {
    /// Initialize the database as needed by running database migrations.
    ///
    /// # Errors
    /// Return error if the migration failed.
    fn initialize(&self, stash: &Stash) -> Result<(), MigratorError>;

    /// A helper to return a boxed trait object.
    fn boxed(self) -> Box<dyn UserDatabaseInitializer>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    session: Session,
    context: Arc<Context>,
    user_stash: Stash,
    user_id: UserId,
    session_id: AuthId,
    status: Arc<Mutex<ConnectionStatus>>,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    pub images_logo_cache: Arc<ProtonCache<SenderImage>>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn new(
        session: Session,
        context: Arc<Context>,
        user_stash_path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
        user_id: UserId,
        session_id: AuthId,
        cache_path: PathBuf,
        sender_image_cache_size: u64,
    ) -> CoreContextResult<Arc<Self>> {
        let user_stash = Self::new_user_db(user_stash_path, db_initializers).await?;

        let images_logo_cache = Self::init_sender_image_cache(
            cache_path.join("sender_images"),
            sender_image_cache_size,
            &user_stash,
        )
        .await?;

        Ok(Arc::new(Self {
            session,
            context,
            user_stash,
            user_id,
            session_id,
            status: Arc::new(Mutex::new(ConnectionStatus::Online)),
            key_manager: Arc::new(CryptoKeyManager::new()),
            images_logo_cache,
        }))
    }

    async fn init_sender_image_cache(
        cache_path: PathBuf,
        cache_size: u64,
        user_stash: &Stash,
    ) -> CoreContextResult<Arc<ProtonCache<SenderImage>>> {
        let cache = ProtonCache::new(
            cache_path.join("images_logo_cache"),
            cache_size,
            user_stash.to_owned(),
        )
        .await?;

        Ok(Arc::new(cache))
    }

    /// Get the network session.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get the network session converted to a type that accepts this type.
    #[must_use]
    pub fn session_as<T: From<Session>>(&self) -> T {
        T::from(self.session.clone())
    }

    /// Get the database connection.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.user_stash
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Retrieves the current user's account details.
    ///
    /// # Errors
    ///
    /// Returns `CoreContextError` if the account does not exist or if an error occurs
    /// during the database query.
    pub async fn account_details(&self) -> CoreContextResult<AccountDetails> {
        let tether = self.context.account_stash().connection();
        let user_id = self.user_id();
        let account = CoreAccount::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| CoreContextError::AccountMissing(user_id.clone()))?;

        Ok(account.details())
    }

    /// Get the session id of this context.
    #[must_use]
    pub fn session_id(&self) -> &AuthId {
        &self.session_id
    }

    /// Get the connection status of the current user session.
    ///
    /// The method will return the current connection status of the user session.
    /// Underlying it will ping the Proton server with one second timeout to check
    /// if the connection can be established.
    ///
    /// The connection status can be one of the following:
    /// - `ConnectionStatus::Online`: The application is online.
    /// - `ConnectionStatus::Offline`: The application is offline.
    /// - `ConnectionStatus::ServerUnreachable`: The application is online but the server is unreachable.
    ///
    /// # Errors
    /// When the connection status cannot be determined which in most cases would be a bug.
    ///
    pub async fn connection_status(&self) -> ConnectionStatus {
        let guard = self.status.lock().await;
        let value = *guard;
        drop(guard);
        let status = Arc::clone(&self.status);
        let session = self.session().clone();

        tokio::task::spawn(async move {
            let response = session.api().ping(Some(ONE_SECOND_TIMEOUT), None).await;
            let mut task_guard = status.lock().await;

            if response.is_err() {
                let error = response.unwrap_err();

                if error.is_server_unreachable() {
                    *task_guard = ConnectionStatus::ServerUnreachable;
                } else if error.is_network_failure() {
                    *task_guard = ConnectionStatus::Offline;
                } else {
                    tracing::error!("Error while pinging the server: {error}");
                }
            } else {
                *task_guard = ConnectionStatus::Online;
            }
        });

        value
    }

    async fn new_user_db(
        path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
    ) -> Result<Stash, MigratorError> {
        let stash = Stash::new(Some(path))?;
        debug!("initializing core database");
        // initialize core db
        migrate_account_db(&stash).await?;
        migrate_core_db(&stash).await?;
        debug!("initializing user ");
        // initialize user db
        for initializer in db_initializers {
            initializer.initialize(&stash)?;
        }

        Ok(stash)
    }
}
