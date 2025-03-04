pub use self::keys::*;
use crate::async_task::{spawn_task, AsyncTaskResult, DefaultTaskSpawner, TaskSpawner};
use crate::cache::ProtonCache;
use crate::datatypes::AccountDetails;
use crate::db::account::CoreAccount;
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::models::sender_image_cache::SenderImage;
use crate::{Context, CoreContextError, CoreContextResult};
use action_queue::ActionQueueContext;
use proton_api_core::connection_status::ConnectionStatus;
use proton_api_core::services::proton::common::{AuthId, UserId};
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::{Stash, StashConfiguration};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub mod action_queue;
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
pub struct UserContext {
    session: Session,
    context: Arc<Context>,
    user_stash: Stash,
    queue_context: ActionQueueContext,
    user_id: UserId,
    session_id: AuthId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    pub images_logo_cache: Arc<ProtonCache<SenderImage>>,
    cancellation_token: CancellationToken,
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
        let cancellation_token = context.new_child_cancellation_token();
        let queue = ActionQueueContext::new(user_stash.clone()).await?;
        let this = Arc::new(Self {
            session,
            context,
            user_stash,
            queue_context: queue,
            user_id,
            session_id,
            key_manager: Arc::new(CryptoKeyManager::new()),
            images_logo_cache,
            cancellation_token,
        });
        let this_weak = Arc::downgrade(&this);

        this.queue().register_execution_context(this_weak);

        Ok(this)
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

    /// Get `ActionQueue` instance.
    #[must_use]
    pub fn queue(&self) -> &ActionQueueContext {
        &self.queue_context
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
    pub async fn connection_status(&self) -> ConnectionStatus {
        self.session.status().await
    }

    async fn new_user_db(
        path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
    ) -> Result<Stash, MigratorError> {
        let stash = Stash::new(StashConfiguration {
            path: Some(path),
            ..Default::default()
        })?;
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

    /// Spawn an async `task` associated to this context.
    ///
    /// See [`spawn_task()`] for more details.
    pub fn spawn<F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        <F as Future>::Output: Send + 'static,
        F: Future + Send + 'static,
    {
        self.spawn_with::<_, DefaultTaskSpawner>(task)
    }

    /// Spawn an async `task` associated to this context with a specific [`TaskSpawner`].
    ///
    /// See [`spawn_task()`] for more details.
    pub fn spawn_with<F, S>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send + 'static,
        S: TaskSpawner + 'static,
    {
        let token = self.cancellation_token.clone();
        spawn_task::<_, S>(token, task)
    }

    /// Cancel all tasks which are bound to this context.
    pub fn cancel_all_tasks(&self) {
        self.cancellation_token.cancel();
    }
}
