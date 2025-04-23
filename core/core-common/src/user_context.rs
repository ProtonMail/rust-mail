pub use self::keys::*;
use crate::datatypes::AccountDetails;
use crate::db::account::{CoreAccount, CoreSession};
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::models::{InitializationWatcher, ModelExtension, UserSettings};
use crate::{Context, CoreContextError, CoreContextResult};
use anyhow::Context as _;
use futures::StreamExt;
use proton_action_queue::queue::Queue;
use proton_api_core::connection_status::ConnectionStatus;
use proton_api_core::services::proton::{SessionId, UserId};
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use proton_task_service::{AsyncTaskResult, DefaultTaskSpawner, TaskSpawner};
use stash::orm::Model;
use stash::stash::{Stash, StashConfiguration, WatcherHandle};
use std::fmt::{Debug, Formatter};
use std::fs::{self};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

pub mod action_queue;
pub mod images_logo;
mod keys;
pub mod nuke_utils;

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
    queue: Queue,
    user_id: UserId,
    session_id: SessionId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    cancellation_token: CancellationToken,
    pub cache_path: PathBuf,
    pub initialization_watcher: Arc<InitializationWatcher>,
    pub hook_sender: watch::Sender<(SessionId, UserId)>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(name = "NewUserContext", skip_all)]
    pub(crate) async fn new(
        session: Session,
        context: Arc<Context>,
        user_stash_path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
        user_id: UserId,
        session_id: SessionId,
        cache_path: PathBuf,
    ) -> CoreContextResult<Arc<Self>> {
        let user_stash = Self::new_user_db(user_stash_path, db_initializers).await?;
        let cancellation_token = context.new_child_cancellation_token();
        let queue = Queue::new(user_stash.clone()).await?;
        let initialization_watcher = InitializationWatcher::new(&user_stash)?;
        let (hook_sender, _) = watch::channel((session_id.clone(), user_id.clone()));
        let hook_sender_clone = hook_sender.clone();
        let this = Arc::new(Self {
            session,
            context,
            user_stash,
            queue,
            user_id,
            session_id,
            key_manager: Arc::new(CryptoKeyManager::new()),
            cache_path,
            cancellation_token,
            initialization_watcher,
            hook_sender,
        });
        let this_weak = Arc::downgrade(&this);

        fs::create_dir_all(this.sender_images_cache_path())
            .context("Error creating sender image cache path")?;

        this.queue().register_execution_context(this_weak);

        fs::create_dir_all(this.sender_images_cache_path())?;
        fs::create_dir_all(this.trash_path())?;

        let init_watcher = this.initialization_watcher.clone();
        this.spawn(async move {
            if let Err(e) = init_watcher.task().await {
                error!("Initialization watcher finished with error: {e:?}");
            }
        });
        let clone_of_this = this.clone();
        let handle = CoreSession::watch(this.context.account_stash())?;

        this.spawn(
            async move { session_cleanup_task(handle, clone_of_this, hook_sender_clone).await },
        );

        Ok(this)
    }

    /// Subscribes for the event of closing the session. Use it to cleanup any remaining tasks
    /// or memory footprints.
    ///
    pub fn on_session_close_hook<H, Fut>(&self, hook: H)
    where
        H: FnOnce(SessionId, UserId) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let mut receiver = self.hook_sender.subscribe();
        // We are not using cancellable futures here because it wont hurt if for any reason it outlives the context.
        // Worst case we will return error because sender was already dropped.
        tokio::task::spawn(async move {
            if receiver.changed().await.is_err() {
                tracing::error!("Sender was dropped before handling this hook");
                return;
            }
            let (session_id, user_id) = receiver.borrow_and_update().clone();
            hook(session_id, user_id).await;
        });
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
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Get path to the log file.
    #[must_use]
    pub fn get_log_path(&self) -> Option<&Path> {
        self.context.get_log_path()
    }

    /// Get path to the log file.
    #[must_use]
    pub fn get_user_db_path(&self) -> &Path {
        self.context.get_user_db_location()
    }

    /// Retrieves the current user's account details.
    ///
    /// # Errors
    ///
    /// Returns `CoreContextError` if the account does not exist or if an error occurs
    /// during the database query.
    pub async fn account_details(&self) -> CoreContextResult<AccountDetails> {
        let account = self.core_account().await?;
        Ok(account.details())
    }

    /// Retrieves the user's settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn user_settings(&self) -> CoreContextResult<UserSettings> {
        let user_id = self.user_id();
        let tether = self.context.account_stash().connection();
        let settings = UserSettings::load(user_id.to_owned(), &tether).await?;

        settings.ok_or_else(|| CoreContextError::SettingsMissing(user_id.to_owned()))
    }

    /// Retrieves the current user's account.
    ///
    /// # Errors
    ///
    /// Returns `CoreContextError` if the account does not exist or if an error occurs
    /// during the database query.
    pub async fn core_account(&self) -> CoreContextResult<CoreAccount> {
        let tether = self.context.account_stash().connection();
        let user_id = self.user_id();
        let account = CoreAccount::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| CoreContextError::AccountMissing(user_id.clone()))?;

        Ok(account)
    }

    /// Get the session id of this context.
    #[must_use]
    pub fn session_id(&self) -> &SessionId {
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

        self.context
            .task_service()
            .spawn_cancellable_with::<S, _>(token, task)
    }

    /// Cancel all tasks which are bound to this context.
    pub fn cancel_all_tasks(&self) {
        self.cancellation_token.cancel();
    }

    #[must_use]
    pub fn sender_images_cache_path(&self) -> PathBuf {
        self.cache_path.join("sender_images")
    }

    #[must_use]
    pub fn trash_path(&self) -> PathBuf {
        self.cache_path.join("trash")
    }

    /// Deletes all of the provided files by first moving them to a temp directory, to be deleted
    /// again next time this is called.
    pub fn delete_files_safe<P: AsRef<Path>>(
        &self,
        files: impl IntoIterator<Item = P>,
    ) -> Result<(), DeleteFilesSafeError> {
        let path = self.trash_path();
        let mut failures = vec![];
        for from in files {
            let from = from.as_ref();
            let Some(filename) = from.file_name() else {
                warn!("No file name");
                failures.push((io::ErrorKind::Other.into(), from.to_owned()));
                continue;
            };

            if let Err(e) = fs::rename(from, path.join(filename)) {
                if e.kind() == io::ErrorKind::NotFound {
                    warn!("Attempting to delete a file that does not exist");
                } else {
                    failures.push((e, from.to_owned()));
                }
            }
        }

        // Best effort remove files even if some failed
        let res = remove_files(&path);
        if !failures.is_empty() {
            Err(DeleteFilesSafeError::Failed(failures))
        } else if let Err(e) = res {
            Err(DeleteFilesSafeError::Moved(e))
        } else {
            Ok(())
        }
    }
}

fn remove_files(path: &Path) -> io::Result<()> {
    for child in fs::read_dir(path)? {
        let child = child?;
        if child.file_type()?.is_dir() {
            fs::remove_dir_all(child.path())?;
        } else {
            fs::remove_file(child.path())?;
        }
    }
    Ok(())
}

/// The errors tha can happen when deleting files
#[derive(Debug)]
pub enum DeleteFilesSafeError {
    /// Could not delete some of the files, maybe try again?
    Failed(Vec<(io::Error, PathBuf)>),

    /// Not all files could be deleted. Next time they probably will.
    Moved(io::Error),
}

#[tracing::instrument(skip_all)]
async fn session_cleanup_task(
    handle: WatcherHandle,
    this: Arc<UserContext>,
    hook_sender: watch::Sender<(SessionId, UserId)>,
) -> CoreContextResult<()> {
    let mut receiver = handle.receiver.into_stream();
    tracing::debug!("Starting cleanup task");
    while receiver.next().await.is_some() {
        tracing::debug!("Detected change in core_session table");
        let tether = this.context.account_stash().connection();
        let maybe_session = CoreSession::find_by_id(this.session_id.clone(), &tether).await?;
        if maybe_session.is_none() {
            tracing::warn!("Core session for {:?} not found.", this.session_id);
            tracing::warn!("Clearing tasks...");
            // Core session has been deleted.
            // Clearup
            _ = hook_sender.send((this.session_id.clone(), this.user_id.clone()));
            this.cancel_all_tasks();
            break;
        }
    }
    tracing::warn!("User context cleanup task has ended");
    Ok::<_, CoreContextError>(())
}
