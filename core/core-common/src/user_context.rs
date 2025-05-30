pub use self::keys::*;
use crate::datatypes::AccountDetails;
use crate::datatypes::Refresh;
use crate::db::account::CoreAccount;
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::events::{Action, AddressEvent, ContactEmailEvent, ContactEvent, CoreEvent};
use crate::models::Address;
use crate::models::Contact;
use crate::models::Label;
use crate::models::ModelExtension;
use crate::models::{InitializationWatcher, User, UserSettings};
use crate::{Context, CoreContextError, CoreContextResult, OnSessionDeletedResponse};
use anyhow::Context as _;
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use proton_action_queue::queue::Queue;
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::services::proton::{EventId, ProtonCore, SessionId, UserId};
use proton_core_api::session::{CoreSession, Session};
use proton_event_loop::RawEvent;
use proton_event_loop::foreground_loop::EventLoop;
use proton_event_loop::provider::Provider;
use proton_event_loop::store::Store;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use proton_sqlite3::MigratorError;
use proton_task_service::{AsyncTaskResult, DefaultTaskSpawner, TaskSpawner};
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;
use stash::stash::{Bond, Stash, StashConfiguration};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs::{self};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub mod action_queue;
pub mod images_logo;
mod keys;
pub mod nuke_utils;

const CORE_EVENT_TYPE_ID: &str = "proton-core-event";

/// Event loop context for core events
#[derive(Clone)]
pub struct CoreEventLoopContext(Weak<UserContext>);

impl CoreEventLoopContext {
    pub fn inner(&self) -> Result<Arc<UserContext>, anyhow::Error> {
        match self.0.upgrade() {
            Some(ctx) => Ok(ctx),
            None => bail!("UserContext no longer alive"),
        }
    }

    #[must_use]
    pub fn boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }
}

impl From<Weak<UserContext>> for CoreEventLoopContext {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl Store for CoreEventLoopContext {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        let ctx = self.inner()?;
        let tether = ctx.stash().connection();
        match tether
            .query_value::<_, EventId>(
                "SELECT value FROM event_id_store WHERE id = ?1",
                params![CORE_EVENT_TYPE_ID],
            )
            .await
        {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    error!("Failed to load core event id from db:{e:?}");
                    Err(anyhow!("Failed to load core event id {e}"))
                }
            }
        }
    }

    async fn store(&self, id: EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        ctx.stash()
            .connection()
            .tx(async |tx| {
                tx.execute(
                    "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                    params![CORE_EVENT_TYPE_ID, id],
                )
                .await?;

                Ok(())
            })
            .await
            .map_err(|e: StashError| {
                error!("Failed to store core event id in db:{e:?}");
                anyhow!("Failed to store core event id {e}")
            })
    }
}

#[async_trait]
impl Provider for CoreEventLoopContext {
    async fn get_latest_event_id(
        &self,
    ) -> Result<EventId, proton_core_api::service::ApiServiceError> {
        let ctx = self.inner()?;
        Ok(ctx.session().api().get_events_latest().await?.event_id)
    }

    async fn get_event(
        &self,
        event_id: &EventId,
    ) -> Result<RawEvent, proton_core_api::service::ApiServiceError> {
        let ctx = self.inner()?;
        let json_string = ctx
            .session()
            .api()
            .get_event(
                event_id.clone(),
                proton_core_api::services::proton::GetEventOptions::default(),
            )
            .await?;

        Ok(RawEvent::from_json(json_string)?)
    }
}

#[async_trait]
impl Subscriber<CoreEvent> for CoreEventLoopContext {
    fn name(&self) -> &'static str {
        "proton-core-event-subscriber"
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(self, events))]
    async fn on_events(&self, events: &mut [CoreEvent]) -> Result<(), SubscriberError> {
        let ctx = self.inner()?;
        let user_id = ctx.user_id().clone();
        let stash = ctx.stash().clone();

        let mut conn = stash.connection();
        conn.tx::<_, _, StashError>(async |tx| {
            for event in events.iter_mut() {
                if let Some(user) = event.get_core_event_user_mut() {
                    debug!("Handling user event");
                    user.save(tx).await.map_err(|e| {
                        error!("Failed to update user: {e:?}");
                        e
                    })?;
                }
                if let Some(settings) = event.get_core_event_user_settings_mut() {
                    debug!("Handling user setting event");
                    settings.remote_id = Some(user_id.clone());
                    settings.save(tx).await.map_err(|e| {
                        error!("Failed to update user settings:{e:?}");
                        e
                    })?;
                }
                if let Some(used_space) = event.get_core_event_used_space() {
                    debug!("Handling user space event");
                    let mut user = User::load(user_id.clone(), tx).await?.unwrap();
                    user.used_space = used_space;
                    user.save(tx).await.map_err(|e| {
                        error!("Failed to update used space:{e:?}");
                        e
                    })?;
                }
                if let Some(used_product_space) = event.get_core_event_used_product_space() {
                    debug!("Handling user product space event");
                    let mut user = User::load(user_id.clone(), tx).await?.unwrap();
                    user.product_used_space = used_product_space.clone();
                    user.save(tx).await.map_err(|e| {
                        error!("Failed to update used space:{e:?}");
                        e
                    })?;
                }
                if let Some(addresses) = event.get_core_event_addresses_mut() {
                    debug!("Handling address event");
                    handle_address_event(tx, addresses).await?;
                }
                if let Some(contacts) = event.get_core_event_contacts_mut() {
                    debug!("Handling contact events");
                    handle_contact_event(tx, contacts).await?;
                }
                if let Some(contact_emails) = event.get_core_event_contact_emails_mut() {
                    debug!("Handling contact email events");
                    handle_contact_email_event(tx, contact_emails).await?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e: StashError| SubscriberError::Other(anyhow!("Failed apply changes: {e}")))
    }

    async fn on_refresh(&self, event: &CoreEvent) -> Result<(), SubscriberError> {
        let ctx = self.inner()?;

        ctx.on_refresh_impl(event.refresh).await
    }
}

impl UserContext {
    pub async fn on_refresh_impl(
        self: &Arc<Self>,
        refresh: Refresh,
    ) -> Result<(), SubscriberError> {
        info!("Handling refresh event: {refresh:?}");
        let ctx = self;

        macro_rules! try_refresh {
            ($fn_name:tt) => {{
                let max_attempts = 2;
                let mut attempts = 0;

                while let Err(e) = $fn_name(ctx.clone()).await {
                    if attempts >= max_attempts {
                        return Err(e);
                    }
                    attempts += 1;
                    warn!("Refresh event attempt {attempts} failed: `{e}`");
                }
            }};
        }

        match refresh {
            Refresh::None => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
            }
            Refresh::Contacts => {
                try_refresh!(refresh_contacts);
            }
            Refresh::Mail => {
                // Mail refresh is handled by the mail context
            }
            Refresh::All => {
                try_refresh!(refresh_core);
            }
            Refresh::Unknown(other) => {
                warn!("Unknown refresh event type: {other}");
            }
        }

        Ok(())
    }
}

macro_rules! join_task {
    ($name:tt, $description: expr) => {{
        if let AsyncTaskResult::Completed(Ok(value)) = $name
            .await
            .map_err(|e| anyhow!("Failed to download remote {}: `{e}`", $description))?
        {
            value
        } else {
            return Err(SubscriberError::Other(anyhow!(
                "The task `{}` was cancelled, we need to run refresh again",
                $description
            )));
        }
    }};
}

#[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
async fn refresh_core(ctx: Arc<UserContext>) -> Result<(), SubscriberError> {
    let api = ctx.session().api().clone();
    let contacts = ctx.spawn(async move { Contact::sync(&api).await });
    let api = ctx.session().api().clone();
    let all_remote_addresses = ctx.spawn(async move { Address::sync(&api).await });
    let api = ctx.session().api().clone();
    let user_and_settings = ctx.spawn(async move { User::sync_user_and_settings(&api).await });
    let api = ctx.session().api().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });

    let mut tether = ctx.stash().connection();
    let mut all_local_addresses: HashMap<_, _> = Address::all(&tether)
        .await?
        .into_iter()
        .map(|addr| (addr.remote_id.clone(), addr))
        .collect();
    let mut all_local_labels: HashMap<_, _> = Label::all_contact_groups(&tether)
        .await?
        .into_iter()
        .map(|label| (label.remote_id.clone(), label))
        .collect();
    debug!(
        "Number of labels available localy: {}",
        all_local_labels.len()
    );

    debug!(
        "Number of addresses available localy: {}",
        all_local_addresses.len()
    );

    let all_remote_addresses = join_task!(all_remote_addresses, "addresses").inner();
    let user_and_settings = join_task!(user_and_settings, "user and settings");
    let all_remote_labels = join_task!(all_remote_labels, "labels");

    debug!(
        "Number of addresses available remotely: {}",
        all_remote_addresses.len()
    );
    for remote_label in &all_remote_addresses {
        all_local_addresses.remove(&remote_label.remote_id);
    }
    debug!(
        "Number of labels available remotely: {}",
        all_remote_labels.len()
    );
    for remote_label in &all_remote_labels {
        all_local_labels.remove(&remote_label.remote_id);
    }

    let contacts = join_task!(contacts, "contacts");

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            for local_address_to_remove in all_local_addresses.into_values() {
                debug!(
                    "Removing address with remote_id {:?}",
                    local_address_to_remove.remote_id
                );
                local_address_to_remove.delete(tx).await?;
            }
            for mut remote_address in all_remote_addresses {
                remote_address.save(tx).await?;
            }

            Label::sync_labels(tx, all_remote_labels)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to sync labels: {e}");
                    error!("{e:?}");
                    SubscriberError::Other(e)
                })?;

            for local_label_to_remove in all_local_labels.into_values() {
                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                local_label_to_remove.delete(tx).await?;
            }
            user_and_settings.store(tx).await?;
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries while refreshing core: {e}");
        })?;

    Ok(())
}

#[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
async fn refresh_contacts(ctx: Arc<UserContext>) -> Result<(), SubscriberError> {
    let contacts = Contact::sync(ctx.session().api()).await?;
    let mut tether = ctx.stash().connection();

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries while refreshing core: {e}");
        })?;

    Ok(())
}

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
    event_loop: EventLoop<CoreEvent>,
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

        let this = Arc::new_cyclic(|this| {
            let event_ctx = CoreEventLoopContext::from(Weak::clone(this));

            Self {
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
                event_loop: EventLoop::new(event_ctx.boxed(), event_ctx.boxed()),
            }
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

        // Register task cancellation when session is deleted.
        let this_user_id = this.user_id.clone();
        let this_weak = Arc::downgrade(&this);
        this.context.on_session_deleted(move |_, user_id| {
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

        // Register the core event subscriber
        let event_ctx = CoreEventLoopContext::from(Arc::downgrade(&this));
        this.event_loop
            .register(Box::new(event_ctx))
            .await
            .map_err(|e| {
                CoreContextError::Other(anyhow::anyhow!(
                    "Failed to register core event subscriber: {e}"
                ))
            })?;

        Ok(this)
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

    /// Get path to the database file
    #[must_use]
    pub fn get_user_db_path(&self) -> PathBuf {
        self.context.user_db_path(self.user_id())
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

    #[must_use]
    pub fn did_receive_task_cancellation_request(&self) -> bool {
        self.cancellation_token.is_cancelled()
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

/// The errors that can happen when deleting files
#[derive(Debug)]
pub enum DeleteFilesSafeError {
    /// Could not delete some of the files, maybe try again?
    Failed(Vec<(io::Error, PathBuf)>),

    /// Not all files could be deleted. Next time they probably will.
    Moved(io::Error),
}

async fn handle_address_event(
    tx: &Bond<'_>,
    address_events: &mut [AddressEvent],
) -> Result<(), StashError> {
    for event in address_events {
        event.action.log_entry(&event.remote_id);
        match event.action {
            Action::Delete => {
                warn!("[ET-1461] Delete action not implemented for address event");
            }

            Action::Create | Action::Update => {
                if let Some(ref mut address) = event.address {
                    address.save(tx).await?;
                }
            }

            Action::UpdateFlags => {
                warn!("[ET-1461] UpdateFlags action not implemented for address event");
            }
        }
    }

    Ok(())
}

async fn handle_contact_event(
    tx: &Bond<'_>,
    contact_events: &mut [ContactEvent],
) -> Result<(), StashError> {
    for event in contact_events {
        event.action.log_entry(&event.remote_id);
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contacts WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact: {e:?}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact) = event.contact {
                    contact.save(tx).await.map_err(|e| {
                        error!("Failed to create or update contact: {e:?}");
                        e
                    })?;
                }
            }
            Action::UpdateFlags => (),
        }
    }
    Ok(())
}

async fn handle_contact_email_event(
    tx: &Bond<'_>,
    contact_email_events: &mut [ContactEmailEvent],
) -> Result<(), StashError> {
    for event in contact_email_events {
        event.action.log_entry(&event.remote_id);
        match event.action {
            Action::Delete => tx
                .execute(
                    "DELETE FROM contact_emails WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await
                .map(|_| ())
                .map_err(|e| {
                    error!("Failed to delete contact mail: {e:?}");
                    e
                })?,
            Action::Create | Action::Update => {
                if let Some(ref mut contact_email) = event.contact_email {
                    contact_email.save(tx).await.map_err(|e| {
                        error!("Failed to create or update contact mail: {e:?}");
                        e
                    })?;
                }
            }
            Action::UpdateFlags => (),
        }
    }
    Ok(())
}
