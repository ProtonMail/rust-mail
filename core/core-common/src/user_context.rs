pub use self::keys::*;
use self::services::{EventLoopService, InitializationService};

use crate::actions::event_poll::EventPoll as EventPollAction;
use crate::context::services::{SessionObserverService, UserMetricService};
use crate::datatypes::AccountDetails;
use crate::db::account::CoreAccount;
use crate::db::migrations::{migrate_core_db, verify_core_db};
use crate::models::{Address, InitializationWatcher, Label, User, UserSettings};
use crate::services::AddressService;
use crate::{Context, CoreContextError, CoreContextResult, OnSessionDeletedResponse, Origin};
pub use event_loop::subscriber::CoreEventLoopContext;
use proton_action_queue::queue::{self, Queue};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::Session;
use proton_event_loop::EventPoll;
use proton_log_service::LogService;
use proton_sqlite3::MigratorError;
use services::{PaymentsService, UserFeatureFlagsService};
use stash::orm::Model;
use stash::stash::{Stash, StashConfiguration, StashError, WatcherHandle};
use stash::watcher::TableWatcher;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs::{self};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use crate::services::user_issue_reporter_service::UserIssueReporterService;
use anyhow::anyhow;
use proton_core_api::connection_status::ConnectionStatus;
use proton_issue_reporter_service::{IssueLevel, IssueReportKeys};
use tokio::task::{self, JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub mod action_queue;
pub mod builder;
pub mod event_loop;
pub mod images_logo;
mod keys;
pub mod nuke_utils;
pub mod services;

#[async_trait::async_trait]
pub trait UserDatabaseInitializer: Send + Sync {
    async fn initialize(&self, stash: &Stash) -> Result<(), MigratorError>;

    fn boxed(self) -> Box<dyn UserDatabaseInitializer>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Contains all the relevant information to an initialize user session.
pub struct UserContext {
    this: Weak<Self>,
    context: Arc<Context>,
    // Context data
    user_id: UserId,
    session_id: SessionId,
    cache_path: PathBuf,
    // Essential services
    session: Session,
    user_stash: Stash,
    queue: Queue,
    key_manager: Arc<CryptoKeyManager>,
    cancellation_token: CancellationToken,
    services: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl Drop for UserContext {
    fn drop(&mut self) {
        let user_id = self.user_id();
        let session_id = self.session_id();
        tracing::info!(?user_id, ?session_id, "Dropping UserContext");
        self.cancellation_token.cancel();
    }
}

impl UserContext {
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    #[tracing::instrument(name = "NewUserContext", skip_all, fields(user_id=%user_id))]
    pub(crate) async fn new(
        session: Session,
        context: Arc<Context>,
        user_stash_path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
        user_id: UserId,
        session_id: SessionId,
        cache_path: PathBuf,
    ) -> CoreContextResult<Arc<Self>> {
        info!("Creating new UserContext");
        let issue_reporter = context.issue_reporter_service();
        let user_issue_reporter = issue_reporter
            .reporter()
            .new_user_reporter(user_id.clone().into_inner());
        let user_issue_reporter_cloned = user_issue_reporter.clone();
        async {
            let user_stash =
                Self::open_db(user_stash_path, db_initializers, context.origin()).await?;
            let cancellation_token = context.new_child_cancellation_token();
            let queue = Queue::new(user_stash.clone()).await?;

            let origin = context.origin();
            let context_cloned = context.clone();
            let cancellation_token_cloned = cancellation_token.clone();

            // There was bug in previous versions that would allow the user to submit unlimited
            // amount of event polls when using pull to refresh. We saw certain users with
            // up to 100 pending event polls. We have address this in this patch, but
            // we still need clean these up.
            // There is no harm in doing this at startup since we will queue a new one
            // on enter foreground.
            let deleted_event_polls = queue.delete_all_by_type::<EventPollAction>().await?;
            tracing::info!("Deleted {deleted_event_polls} event actions");

            let this = {
                let mut builder = builder::UserContextBuilder::new();
                builder = builder
                    .with_cyclic_service(|weak| {
                        UserIssueReporterService::new(weak, user_issue_reporter)
                    })
                    .with_cyclic_service(AddressService::new);

                if matches!(origin, Origin::App) {
                    builder = builder
                        .with_cyclic_service(UserFeatureFlagsService::new)
                        .with_cyclic_service(PaymentsService::new)
                        .with_cyclic_service(move |weak_ref: Weak<UserContext>| {
                            let event_ctx = CoreEventLoopContext::from(weak_ref);
                            let event_loop = EventPoll::new(
                                context_cloned.task_service().task_service(),
                                cancellation_token_cloned,
                                event_ctx.boxed(),
                                event_ctx.boxed(),
                            );
                            EventLoopService::new(event_loop)
                        })
                        .with_service(InitializationService::new(
                            InitializationWatcher::new(&user_stash).await?,
                        ))
                        .with_cyclic_service(UserMetricService::new);
                }

                builder.build(
                    session,
                    context,
                    user_stash,
                    queue,
                    user_id,
                    session_id,
                    Arc::new(CryptoKeyManager::new()),
                    cancellation_token,
                    cache_path,
                )
            };

            fs::create_dir_all(this.sender_images_cache_path())?;
            fs::create_dir_all(this.trash_path())?;

            if matches!(origin, Origin::App)
                && let Some(init_service) = this.get_service_opt::<InitializationService>()
            {
                let init_watcher = init_service.initialization_watcher().clone();
                this.spawn(async move {
                    if let Err(e) = init_watcher.task().await {
                        error!("Initialization watcher finished with error: {e:?}");
                    }
                });
            }

            let this_user_id = this.user_id.clone();
            let this_weak = Arc::downgrade(&this);
            if let Some(session_service) = this.context.get_service_opt::<SessionObserverService>()
            {
                let event_service = this.context.event_service();
                session_service.on_session_deleted(event_service, move |_, user_id| {
                    let this_user_id = this_user_id.clone();
                    let this_weak = this_weak.clone();
                    async move {
                        if user_id == this_user_id {
                            if let Some(ctx) = this_weak.upgrade() {
                                ctx.cancel_all_tasks();
                            }
                            return OnSessionDeletedResponse::Terminate;
                        }
                        OnSessionDeletedResponse::Continue
                    }
                });
            }

            if matches!(origin, Origin::App) {
                this.register_subscribers().await?;
            }

            info!("Creating new UserContext...Done");
            Ok(this)
        }
        .await
        .inspect_err(|e| {
            user_issue_reporter_cloned.report(
                IssueLevel::Critical,
                format!("Failed to create user context: {e:?}"),
                IssueReportKeys::default(),
            );
        })
    }

    #[must_use]
    pub fn as_arc(&self) -> Arc<Self> {
        self.this.upgrade().expect("Should never fail")
    }

    #[must_use]
    pub fn get_service_opt<T: Any + 'static>(&self) -> Option<&T> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|service| service.downcast_ref::<T>())
    }

    #[allow(clippy::result_large_err)]
    /// # Panics
    /// This function panics if the service is not found.
    /// If there is a need for a service that may not exist, use `get_service_opt`.
    #[must_use]
    pub fn get_service<T: Any + 'static>(&self) -> &T {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|service| service.downcast_ref::<T>())
            .unwrap_or_else(|| panic!("Service {} not found", std::any::type_name::<T>()))
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.user_stash
    }

    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[allow(clippy::result_large_err)]
    #[must_use]
    pub fn event_loop_service(&self) -> &EventLoopService {
        self.get_service::<EventLoopService>()
    }

    #[must_use]
    pub fn log_service(&self) -> &LogService {
        self.context.log_service()
    }

    #[must_use]
    pub fn get_user_db_path(&self) -> PathBuf {
        self.context.user_db_path(self.user_id())
    }

    pub async fn account_details(&self) -> CoreContextResult<AccountDetails> {
        let account = self.core_account().await?;
        Ok(account.details())
    }

    pub async fn user_settings(&self) -> CoreContextResult<UserSettings> {
        let user_id = self.user_id();
        let tether = self.stash().connection().await?;
        let settings = UserSettings::load(user_id.to_owned(), &tether).await?;

        settings.ok_or_else(|| CoreContextError::SettingsMissing(user_id.to_owned()))
    }

    pub async fn core_account(&self) -> CoreContextResult<CoreAccount> {
        let tether = self.context.account_stash().connection().await?;
        let user_id = self.user_id();
        let account = CoreAccount::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| CoreContextError::AccountMissing(user_id.clone()))?;

        Ok(account)
    }

    #[must_use]
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[must_use]
    pub fn connection_status(&self) -> ConnectionStatus {
        self.context.network_monitor_service().combined_status()
    }

    async fn open_db(
        path: &Path,
        inits: &[Box<dyn UserDatabaseInitializer>],
        origin: Origin,
    ) -> Result<Stash, MigratorError> {
        let path = path.to_owned();

        let stash = task::spawn_blocking(move || {
            Stash::new(StashConfiguration {
                path: Some(&path),
                ..Default::default()
            })
        })
        .await
        .map_err(|err| MigratorError::Stash(StashError::Custom(anyhow!("{err}"))))??;

        match origin {
            Origin::App => {
                debug!("initializing database");

                migrate_core_db(&stash).await?;

                for init in inits {
                    init.initialize(&stash).await?;
                }
            }

            Origin::ShareExt => {
                debug!("verifying database");

                verify_core_db(&stash).await?;
            }
        }

        Ok(stash)
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

        self.context.task_service().spawn_cancellable(token, task)
    }

    #[must_use]
    pub fn did_receive_task_cancellation_request(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    pub fn cancel_all_tasks(&self) {
        self.cancellation_token.cancel();
    }

    #[must_use]
    pub fn create_child_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    #[must_use]
    pub fn cache_path(&self) -> &PathBuf {
        &self.cache_path
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

    pub async fn watch_addresses(&self) -> Result<WatcherHandle, StashError> {
        TableWatcher::<Address>::watch(&self.user_stash).await
    }

    pub async fn watch_user(&self) -> Result<WatcherHandle, StashError> {
        TableWatcher::<User>::watch(&self.user_stash).await
    }

    pub async fn watch_user_settings(&self) -> Result<WatcherHandle, StashError> {
        TableWatcher::<UserSettings>::watch(&self.user_stash).await
    }

    pub async fn watch_labels(&self) -> Result<WatcherHandle, StashError> {
        TableWatcher::<Label>::watch(&self.user_stash).await
    }

    #[must_use]
    pub fn issue_reporter_service(&self) -> &UserIssueReporterService {
        self.get_service::<UserIssueReporterService>()
    }

    #[must_use]
    pub fn address_service(&self) -> &AddressService {
        self.get_service::<AddressService>()
    }

    #[must_use]
    pub fn feature_flags(&self) -> &UserFeatureFlagsService {
        self.get_service::<UserFeatureFlagsService>()
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

/// The errors that can happen when deleting files
#[derive(Debug)]
pub enum DeleteFilesSafeError {
    /// Could not delete some of the files, maybe try again?
    Failed(Vec<(io::Error, PathBuf)>),

    /// Not all files could be deleted. Next time they probably will.
    Moved(io::Error),
}

impl queue::TaskSpawner for UserContext {
    fn spawn_task<F>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.context
            .task_service()
            .spawn_cancellable(self.cancellation_token.clone(), future)
    }
}
