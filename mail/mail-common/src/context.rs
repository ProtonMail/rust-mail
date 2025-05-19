use crate::actions::MailActionError;
use crate::{AppError, MailUserContext, draft};
use futures::executor::block_on;
use proton_action_queue::action::{Action, WriterGuardError};
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedError};
use proton_calendar_common::RsvpError;
use proton_core_api::login::{Flow, LoginError};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::BuildError;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::Config;
use proton_core_api::status_watcher::StatusWatcher;
use proton_core_api::verification::DynChallengeNotifier;
use proton_core_common::UserDatabaseInitializer;
use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::models::LabelError;
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::{
    ContactError, Context, CoreAccountState, CoreContextError, CoreSessionState, KeyHandlingError,
    UserContext,
};
use proton_crypto_inbox::attachment::AttachmentEncryptionError;
use proton_crypto_inbox::keys::EncryptionPreferencesError;
use proton_event_loop::EventLoopError;
use proton_sqlite3::MigratorError;
use proton_task_service::{AsyncTaskResult, TaskSpawner};
use stash::stash::{Stash, StashError, WatcherHandle};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::{JoinError, JoinHandle};

/// Whether we should initialize MailUserContext on creation
#[derive(Debug, Clone, Copy)]
pub enum ShouldInitializeMailUserContext {
    /// When creating user context, it should be also initialized.
    /// Initialization means calling APIs for the data which might fail.
    Yes,

    /// # Caution
    /// Used only for tests - we want to postpone initialization but still
    /// have an access to uninitialized context - in order to setup data for mocks etc.
    No,
}

/// Errors that may occur while interacting with a MailContext.
#[derive(Debug, thiserror::Error)]
pub enum MailContextError {
    #[error("Session with id {0} is missing in the DB")]
    SessionMissing(SessionId),
    #[error("Account with user id {0} is missing in the DB")]
    AccountMissing(UserId),
    #[error("Settings for user with id {0} are missing in the DB")]
    SettingsMissing(UserId),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error(transparent)]
    AttachmentEncryption(#[from] AttachmentEncryptionError),
    #[error("Build Error: {0}")]
    Build(#[from] BuildError),
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] MigratorError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::queue::Error),
    #[error("Action: {0}")]
    Action(#[from] MailActionError),
    #[error("QueuedAction: {0}")]
    QueuedAction(#[from] QueuedError),
    #[error("Failed to access OpenPGP keys: {0}")]
    PGPKeyAccess(KeyHandlingError),
    #[error("Failed to select OpenPGP keys for encryption: {0}")]
    PGPKeySelection(#[from] EncryptionPreferencesError),
    #[error("Label Error: {0}")]
    Label(#[from] LabelError),
    #[error("App Error: {0}")]
    App(#[from] AppError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Login Error: {0}")]
    Login(#[from] LoginError),
    #[error("API Error: {0}")]
    Api(#[from] ApiServiceError),
    #[error("Problem with loading contact: {0}")]
    ContactError(#[from] ContactError),
    #[error("Draft: {0}")]
    Draft(#[from] draft::Error),
    #[error("Attempting to create more than one context for the user with id {0}")]
    DuplicateContext(UserId),
    #[error("The context instance is missing")]
    MissingContext,
    #[error("The user context for {0} is not initialized")]
    UserContextNotInitialized(UserId),
    #[error("A task was cancelled")]
    TaskCancelled,
    #[error("Queue Write Guard Expired")]
    QueueWriterGuardExpired,
    #[error("Bug: Called fetch_attachment_data on a pgp attachment.")]
    CalledFetchedAttachmentOnPgp,
    #[error(
        "Bug: Called fetch_attachment_data on an attachment without remoteid (local attachment)."
    )]
    CalledFetchedAttachmentLocalAttachment,
    #[error("Bug: Invalid utf8 somewhere in path: {0:?}.")]
    InvalidUtf8AttachmentPath(std::ffi::OsString),
    #[error("Could not start transaction: {0}")]
    IntoTransactionError(anyhow::Error),
    #[error("Communication error with init mediator")]
    InitMediatorError,
    #[error(transparent)]
    Rsvp(#[from] RsvpError),
    #[error("{0}")]
    Other(anyhow::Error),
}

impl MailContextError {
    pub fn no_connection() -> Self {
        Self::Api(ApiServiceError::NetworkError("No connection".to_string()))
    }
}

impl proton_action_queue::action::Error for MailContextError {
    fn is_network_failure(&self) -> bool {
        if let Self::Api(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        if let Self::IntoTransactionError(err) = self {
            if let Some(WriterGuardError::Expired) = err.downcast_ref() {
                return true;
            }
        }
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

impl From<WriterGuardError> for MailContextError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => MailContextError::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => MailContextError::Stash(e),
        }
    }
}

impl From<CoreContextError> for MailContextError {
    fn from(value: CoreContextError) -> Self {
        match value {
            CoreContextError::AccountMissing(id) => MailContextError::AccountMissing(id),
            CoreContextError::SettingsMissing(id) => MailContextError::SettingsMissing(id),
            CoreContextError::Build(err) => MailContextError::Build(err),
            CoreContextError::Login(err) => MailContextError::Login(err),
            CoreContextError::Api(err) => MailContextError::Api(err),
            CoreContextError::Crypto => MailContextError::Crypto,
            CoreContextError::KeyChain(err) => MailContextError::KeyChain(err),
            CoreContextError::IO(err) => MailContextError::IO(err),
            CoreContextError::DBMigration(err) => MailContextError::DBMigration(err),
            CoreContextError::KeyChainHasNoKey => MailContextError::KeyChainHasNoKey,
            CoreContextError::Other(err) => MailContextError::Other(err),
            CoreContextError::PGPKeyAccess(err) => MailContextError::PGPKeyAccess(err),
            CoreContextError::Stash(err) => MailContextError::Stash(err),
            CoreContextError::ContactError(err) => MailContextError::ContactError(err),
            CoreContextError::DuplicateContext(user_id) => Self::DuplicateContext(user_id),
            CoreContextError::QueueWriterGuardExpired => Self::QueueWriterGuardExpired,
            CoreContextError::Action(core_action_error) => Self::Action(core_action_error.into()),
            CoreContextError::QueuedAction(queued_error) => Self::QueuedAction(queued_error),
            CoreContextError::ActionQueue(error) => Self::ActionQueue(error),
        }
    }
}

pub type MailContextResult<T> = Result<T, MailContextError>;

impl<T: Action<Error: Into<MailContextError>>> From<QueueActionError<T>> for MailContextError {
    fn from(value: QueueActionError<T>) -> Self {
        match value {
            QueueActionError::Action(e) => e.into(),
            QueueActionError::Queue(e) => Self::ActionQueue(e),
        }
    }
}

impl From<JoinError> for MailContextError {
    fn from(value: JoinError) -> Self {
        Self::Other(anyhow::Error::new(value))
    }
}

/// Defines how the event loop should be polled
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum EventPollMode {
    /// On demand,
    Manual,
    /// Background task that queues a request to polls the event loop in the
    /// specified duration.
    Automatic(Duration),
}

pub struct MailContext {
    core_context: Arc<Context>,
    mail_cache_path: PathBuf,
    /// This will get used in the near future.
    pub attachment_cache_size: u64,
    active_user_contexts: Mutex<HashMap<UserId, Weak<MailUserContext>>>,
    pub event_poll_mode: EventPollMode,
}

impl MailContext {
    /// Create a new mail context.
    ///
    /// Note this function currently also creates a core context.
    ///
    /// # Error
    ///
    /// Returns error if the context creation failed.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        core_cache_path: impl Into<PathBuf>,
        mail_cache_path: impl Into<PathBuf>,
        cache_size: u64,
        connection_pool_size: Option<u32>,
        key_chain: Arc<dyn KeyChain>,
        api_config: Config,
        hv_notifier: Option<DynChallengeNotifier>,
        log_path: Option<PathBuf>,
        event_poll_mode: EventPollMode,
    ) -> Result<Arc<Self>, MailContextError> {
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(MailUserDatabaseInitializer {})];

        let core_context = Context::new(
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            api_config,
            hv_notifier,
            core_cache_path,
            connection_pool_size,
            log_path,
        )
        .await?;

        Ok(Arc::new(Self {
            core_context,
            mail_cache_path: mail_cache_path.into(),
            attachment_cache_size: cache_size,
            active_user_contexts: Mutex::new(HashMap::new()),
            event_poll_mode,
        }))
    }

    /// Creates MailContext instance based on provided core Context.
    pub async fn new_with_core_context(
        core_context: Arc<Context>,
        mail_cache_path: PathBuf,
        mail_cache_size: u64,
        event_poll_mode: EventPollMode,
    ) -> Result<Arc<Self>, MailContextError> {
        Ok(Arc::new(Self {
            core_context,
            mail_cache_path,
            attachment_cache_size: mail_cache_size,
            active_user_contexts: Mutex::new(HashMap::new()),
            event_poll_mode,
        }))
    }

    /// Begin a login flow.
    ///
    /// This method initiates a new [`Flow`], used to log in to a Proton account.
    /// The flow is used to guide the user through the login process and persist
    /// the resulting session data.
    ///
    /// # Errors
    ///
    /// See [`Context::new_login_flow`].
    pub async fn new_login_flow(&self) -> MailContextResult<Flow> {
        Ok(self.core_context.new_login_flow().await?)
    }

    /// Resume a partially completed login flow.
    ///
    /// The initial [`Flow::login`] call creates a new session in the database.
    /// However, this session may not yet be usable if 2FA or an additional
    /// password is required. If at this point the login flow is interrupted,
    /// the session is left in an incomplete state. This method allows resuming
    /// the flow to complete the login process.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The ID of the user to log in (from [`Flow::user_id`]).
    /// * `session_id` - The ID of the session to resume (from [`Flow::session_id`]).
    ///
    /// # Errors
    ///
    /// See [`Context::resume_login_flow`].
    pub async fn resume_login_flow(
        &self,
        user_id: UserId,
        session_id: SessionId,
    ) -> MailContextResult<Flow> {
        let flow = self
            .core_context
            .resume_login_flow(user_id, session_id)
            .await?;

        Ok(flow)
    }

    /// Create a new context from a login flow.
    ///
    /// # Errors
    /// Returns error if the flow is in an invalid state or there was an issue initializing
    /// the user database.
    pub async fn user_context_from_login_flow(
        self: &Arc<Self>,
        login_flow: &mut Flow,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self
            .core_context
            .user_context_from_login_flow(login_flow)
            .await?;

        Arc::clone(self)
            .new_user_context(ctx, ShouldInitializeMailUserContext::Yes)
            .await
    }

    /// Gets new initialized context from existing session.
    ///
    /// It does **NOT** initialize itself. Instead, it returns `None`
    /// if context exists but is not initialized
    ///
    /// # Errors
    ///
    /// Returns an error if we failed to decrypt the user session
    /// or access the user database.
    ///
    pub async fn initialized_user_context_from_session(
        self: &Arc<Self>,
        session: &CoreSession,
        status: Option<StatusWatcher>,
    ) -> MailContextResult<Option<Arc<MailUserContext>>> {
        let ctx = self
            .core_context
            .user_context_from_session(session, status)
            .await?;

        self.new_initialized_user_context(ctx).await
    }

    /// Create a new context from an existing session.
    ///
    /// # Errors
    /// Returns error if we failed to decrypt the user session or access the user database.
    pub async fn user_context_from_session(
        self: &Arc<Self>,
        session: &CoreSession,
        status: Option<StatusWatcher>,
        init: ShouldInitializeMailUserContext,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self
            .core_context
            .user_context_from_session(session, status)
            .await?;

        Arc::clone(self).new_user_context(ctx, init).await
    }

    /// Create all new contexts from all existing sessions.
    ///
    /// It returns `MailUserContext` for each logged in account.
    ///
    /// ### Errors
    ///
    /// When `user_context_from_session` fails or database fails.
    ///
    pub async fn get_all_logged_in_user_ctx(
        self: &Arc<Self>,
    ) -> MailContextResult<Vec<Arc<MailUserContext>>> {
        let sessions = self.get_sessions().await?;
        let mut ctxs = Vec::with_capacity(sessions.len());

        for session in sessions {
            if let CoreSessionState::Authenticated = CoreSessionState::of(&session) {
                ctxs.push(
                    self.user_context_from_session(
                        &session,
                        None,
                        ShouldInitializeMailUserContext::No,
                    )
                    .await?,
                );
            } else {
                tracing::warn!("Found unauthenticated session");
            }
        }

        Ok(ctxs)
    }

    /// Create all new contexts from other existing session.
    ///
    /// It returns `MailUserContext` for each logged in account except one which is
    /// tight to the passed `SessionId`.
    ///
    /// ### Errors
    ///
    /// When `user_context_from_session` fails or database fails.
    ///
    pub async fn get_other_logged_in_user_ctx(
        self: &Arc<Self>,
        current_session_id: &SessionId,
    ) -> MailContextResult<Vec<Arc<MailUserContext>>> {
        let sessions = self.get_sessions().await?;
        let mut ctxs = Vec::with_capacity(sessions.len());

        for session in sessions
            .into_iter()
            .filter(|s| &s.remote_id != current_session_id)
        {
            if let CoreSessionState::Authenticated = CoreSessionState::of(&session) {
                ctxs.push(
                    self.user_context_from_session(
                        &session,
                        None,
                        ShouldInitializeMailUserContext::No,
                    )
                    .await?,
                );
            } else {
                tracing::warn!("Found unauthenticated session");
            }
        }

        Ok(ctxs)
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
    pub async fn get_accounts(&self) -> MailContextResult<Vec<CoreAccount>> {
        Ok(self.core_context.get_accounts().await?)
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
    pub async fn watch_accounts(&self) -> MailContextResult<(Vec<CoreAccount>, WatcherHandle)> {
        Ok(self.core_context.watch_accounts().await?)
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
    pub async fn get_sessions(&self) -> MailContextResult<Vec<CoreSession>> {
        Ok(self.core_context.get_sessions().await?)
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
    pub async fn watch_sessions(&self) -> MailContextResult<(Vec<CoreSession>, WatcherHandle)> {
        Ok(self.core_context.watch_sessions().await?)
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
    ) -> MailContextResult<Vec<CoreSession>> {
        Ok(self.core_context.get_account_sessions(user_id).await?)
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
        user_id: UserId,
    ) -> MailContextResult<(Vec<CoreSession>, WatcherHandle)> {
        Ok(self.core_context.watch_account_sessions(user_id).await?)
    }

    /// Get a single account by its remote (user) ID.
    ///
    /// This is a convenience method that enables retrieving a single account without requiring
    /// the full set of accounts to be loaded first.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account(&self, user_id: UserId) -> MailContextResult<Option<CoreAccount>> {
        Ok(self.core_context.get_account(user_id).await?)
    }

    /// Get the login state of an account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_account_state(
        &self,
        user_id: UserId,
    ) -> MailContextResult<Option<CoreAccountState>> {
        Ok(self.core_context.get_account_state(user_id).await?)
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
    ) -> MailContextResult<Option<CoreSession>> {
        Ok(self.core_context.get_session(session_id).await?)
    }

    /// Get the login state of a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_session_state(
        &self,
        session_id: SessionId,
    ) -> MailContextResult<Option<CoreSessionState>> {
        Ok(self.core_context.get_session_state(session_id).await?)
    }

    /// Get the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn get_primary_account(&self) -> MailContextResult<Option<CoreAccount>> {
        Ok(self.core_context.get_primary_account().await?)
    }

    /// Set the account considered to be the primary account.
    ///
    /// # Errors
    ///
    /// Returns an error if the account is not found.
    pub async fn set_primary_account(&self, user_id: UserId) -> MailContextResult<()> {
        Ok(self.core_context.set_primary_account(user_id).await?)
    }

    /// Logs out all sessions of an account without deleting the account's data.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn logout_account(&self, user_id: UserId) -> MailContextResult<()> {
        Ok(self.core_context.logout_account(user_id).await?)
    }

    /// Removes a user session and deletes all associated data.
    ///
    /// # Errors
    ///
    /// Returns error if the db operation failed. Though it will remove all user data
    /// first, which is non failing operations.
    ///
    pub async fn delete_account(&self, user_id: UserId) -> MailContextResult<()> {
        tracing::warn!("Delete account `{user_id}`");
        self.active_user_contexts.lock().await.remove(&user_id);
        let mail_cache_path = self.mail_cache_path(&user_id);

        Ok(self
            .core_context
            .delete_account(user_id, vec![mail_cache_path])
            .await?)
    }

    /// Path where mail content should be cached for user with `user_id`.
    ///
    pub fn mail_cache_path(&self, user_id: &UserId) -> PathBuf {
        self.mail_cache_path.join(user_id.to_string())
    }

    #[must_use]
    pub fn attachments_cache_path(&self) -> PathBuf {
        self.mail_cache_path.join("attachments")
    }

    /// Get the core context.
    pub fn core_context(&self) -> &Arc<Context> {
        &self.core_context
    }

    /// Get the connection to the session database.
    pub fn session_stash(&self) -> &Stash {
        self.core_context.account_stash()
    }

    /// Retrieve initialized user context or return None.
    ///
    /// Initialized means that we are fully logged in and all the initialization stages
    /// have finished executing.
    async fn new_initialized_user_context(
        self: &Arc<Self>,
        core_context: Arc<UserContext>,
    ) -> Result<Option<Arc<MailUserContext>>, MailContextError> {
        let context = self
            .clone()
            .new_user_context(core_context, ShouldInitializeMailUserContext::No)
            .await?;
        if !context.is_initialized().await? {
            tracing::debug!("Existing context is not initialized");
            return Ok(None);
        }

        Ok(Some(context))
    }

    /// Create a new user context or return an existing one.
    async fn new_user_context(
        self: Arc<Self>,
        core_context: Arc<UserContext>,
        init: ShouldInitializeMailUserContext,
    ) -> Result<Arc<MailUserContext>, MailContextError> {
        let mut active_contexts = self.active_user_contexts.lock().await;

        active_contexts.retain(|_, ctx| ctx.strong_count() != 0);

        if let Some(existing) = active_contexts.get(core_context.user_id()) {
            if let Some(upgraded) = existing.upgrade() {
                // This should be handled by the core context creating,
                // but if for some reason it slips through the cracks,
                // catch it again.
                if upgraded.session_id() != core_context.session_id() {
                    return Err(MailContextError::DuplicateContext(
                        core_context.user_id().clone(),
                    ));
                }
                // Initial context creation might have failed, lets re-initialize.
                // In best case scenario it will just check that it was initialized before and then early exit.
                if matches!(init, ShouldInitializeMailUserContext::Yes) {
                    MailUserContext::initialize_async(upgraded.clone()).await?;
                }
                return Ok(upgraded);
            }
        }

        let ctx = MailUserContext::new(self.clone(), core_context).await?;

        active_contexts.insert(ctx.user_id().clone(), Arc::downgrade(&ctx));

        if matches!(init, ShouldInitializeMailUserContext::Yes) {
            MailUserContext::initialize_async(ctx.clone()).await?;
        }

        Ok(ctx)
    }

    /// See [`Context::spawn()`].
    pub fn spawn<F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.core_context.spawn(task)
    }

    /// See [`Context::spawn_with()`].
    pub fn spawn_with<S, F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        S: TaskSpawner,
        F: Future<Output: Send> + Send + 'static,
    {
        self.core_context.spawn_with::<S, _>(task)
    }

    /// Get all the logged in user context that are active and initialized.
    pub async fn get_all_logged_in_and_initialized_user_contexts(
        self: &Arc<Self>,
    ) -> MailContextResult<Vec<Arc<MailUserContext>>> {
        let sessions = self.get_sessions().await?;
        let mut ctxs = Vec::with_capacity(sessions.len());

        for session in sessions {
            if let CoreSessionState::Authenticated = CoreSessionState::of(&session) {
                match self
                    .initialized_user_context_from_session(&session, None)
                    .await
                {
                    Ok(Some(user_context)) => ctxs.push(user_context),
                    Ok(None) => {
                        tracing::debug!("{} has non-initialized context", session.account_id);
                    }
                    Err(MailContextError::DuplicateContext(user_id)) => {
                        tracing::warn!("Duplicate context detected for {user_id}, skipping");
                    }
                    Err(e) => return Err(e),
                }
            } else {
                tracing::warn!("Found unauthenticated session");
            }
        }

        Ok(ctxs)
    }
}

pub struct MailUserDatabaseInitializer {}

impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    fn initialize(&self, stash: &Stash) -> Result<(), MigratorError> {
        block_on(async {
            crate::db::migrations::migrate_db(stash).await?;
            Ok(())
        })
    }
}
