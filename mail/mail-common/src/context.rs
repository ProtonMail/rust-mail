use crate::actions::MailActionError;
use crate::feature_flags::FeatureFlagsService;
use crate::mail_scroller::MailScrollerError;
use crate::migration_snooper::MailMigrationSnooper;
use crate::{AppError, MailUserContext, draft};
use anyhow::anyhow;
use proton_account_api::login::LoginFlow;
use proton_account_api::shared::challenge::ChallengeInfo;
use proton_account_api::signup::SignupFlow;
use proton_action_queue::action::{self, Action, WriterGuardError};
use proton_action_queue::queue::{
    ActionError as QueueActionError, ActionRequeueReason, QueuedError,
};
use proton_calendar_common::RsvpError;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::BuildError;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::SessionParts;
use proton_core_api::verification::DynChallengeNotifier;
use proton_core_common::auth_store::DecryptExt;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::device::DynDeviceInfoProvider;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::{LabelError, ModelExtension};
use proton_core_common::os::{KeyChain, KeyChainError};
use proton_core_common::pin_code::{PinCode, PinError};
use proton_core_common::post_login_check::DefaultPostLoginValidator;
use proton_core_common::services::{
    DeviceInfoService, NetworkMonitorService, SessionObserverService,
};
use proton_core_common::{
    ContactError, Context, ContextBuilder, CoreAccountState, CoreContextError, CoreContextResult,
    CoreSessionState, KeyHandlingError, Origin, UserContext,
};
use proton_core_common::{OnSessionDeletedResponse, UserDatabaseInitializer};
use proton_crypto_inbox::attachment::AttachmentEncryptionError;
use proton_crypto_inbox::keys::EncryptionPreferencesError;
use proton_event_loop::EventLoopError;
use proton_log_service::LogService;
use proton_network_monitor_service::NetworkMonitorServiceError;
use proton_sqlite3::MigratorError;
use proton_task_service::Spawner;
use secrecy::ExposeSecret;
use stash::stash::{Stash, StashError, WatcherHandle};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use tokio::runtime;
use tokio::sync::Mutex;
use tokio::task::{JoinError, JoinHandle};
use tracing::error;

pub const MAIL_ALLOWED_FREE_USER_COUNT: u64 = 2;
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
    #[error("Pin Error: {0}")]
    Pin(#[from] PinError),
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
    #[error("MailScroller: {0}")]
    MailScroller(#[from] MailScrollerError),
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
    #[error("Lost context")]
    LostContext,
    #[error(transparent)]
    Rsvp(#[from] RsvpError),
    #[error("Error parsing url: {0:?}")]
    UrlParseError(#[from] url::ParseError),
    #[error("One or many pending actions are not processable")]
    NonProcessableActions(QueuedError),
    #[error(transparent)]
    NetworkMonitorService(#[from] NetworkMonitorServiceError),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl MailContextError {
    pub fn no_connection() -> Self {
        Self::Api(ApiServiceError::NetworkError("No connection".to_string()))
    }

    #[must_use]
    pub fn is_network_failure(&self) -> bool {
        match self {
            Self::Api(e) => e.is_network_failure(),
            Self::Draft(draft::Error::Send(draft::SendError::SendMessage(
                draft::PackageError::ModulusRequest(e),
            ))) => e.is_network_failure(),
            _ => false,
        }
    }
}

impl action::Error for MailContextError {
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        if self.is_network_failure() {
            return Some(ActionRequeueReason::NetworkFailed);
        }

        match self {
            Self::IntoTransactionError(err) => {
                if let Some(WriterGuardError::Expired) = err.downcast_ref() {
                    Some(ActionRequeueReason::GuardExpired)
                } else {
                    None
                }
            }

            Self::QueueWriterGuardExpired => Some(ActionRequeueReason::GuardExpired),
            Self::LostContext => Some(ActionRequeueReason::LostContext),

            _ => None,
        }
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
            CoreContextError::EventLoop(err) => Self::EventLoop(err),
            CoreContextError::NetworkMonitorService(e) => Self::NetworkMonitorService(e),
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

pub struct MailContext {
    core_context: Arc<Context>,
    mail_cache_path: PathBuf,
    pub attachment_cache_size: u64,
    active_user_contexts: Mutex<HashMap<UserId, Weak<MailUserContext>>>,
    http_client: OnceLock<reqwest::Client>,
}

impl MailContext {
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument("MailContextNew", skip_all)]
    pub async fn new(
        origin: Origin,
        runtime: runtime::Handle,
        session_db_path: impl Into<PathBuf>,
        user_db_path: impl Into<PathBuf>,
        core_cache_path: impl Into<PathBuf>,
        mail_cache_path: impl Into<PathBuf>,
        cache_size: u64,
        key_chain: Arc<dyn KeyChain>,
        api_config: ApiConfig,
        hv_notifier: Option<DynChallengeNotifier>,
        device_info_provider: Option<DynDeviceInfoProvider>,
        log_service: LogService,
        event_poll_mode: EventPollMode,
        network_monitor_config: proton_network_monitor_service::Config,
    ) -> Result<Arc<Self>, MailContextError> {
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(MailUserDatabaseInitializer {})];

        let core_context_builder =
            ContextBuilder::new().with_cyclic_service(FeatureFlagsService::new);
        let core_context = Context::new(
            core_context_builder,
            origin,
            runtime,
            session_db_path,
            user_db_path,
            key_chain,
            initializers,
            api_config,
            hv_notifier,
            device_info_provider,
            core_cache_path,
            log_service,
            event_poll_mode,
            network_monitor_config,
        )
        .await?;

        let ctx = Arc::new(Self {
            core_context,
            mail_cache_path: mail_cache_path.into(),
            attachment_cache_size: cache_size,
            active_user_contexts: Mutex::new(HashMap::new()),
            http_client: OnceLock::new(),
        });

        let ctx_weak = Arc::downgrade(&ctx);

        if let Some(session_service) = ctx.core_context.get_service_opt::<SessionObserverService>()
        {
            session_service.on_session_deleted(move |_, user_id| {
                let ctx_weak = ctx_weak.clone();
                async move {
                    let Some(ctx) = ctx_weak.upgrade() else {
                        return OnSessionDeletedResponse::Terminate;
                    };

                    tracing::info!("Removing `{user_id}`, from active contexts");
                    ctx.active_user_contexts.lock().await.remove(&user_id);

                    OnSessionDeletedResponse::Continue
                }
            });
        }

        Ok(ctx)
    }

    pub fn feature_flags(&self) -> &FeatureFlagsService {
        self.core_context.get_service::<FeatureFlagsService>()
    }

    pub async fn new_with_core_context(
        core_context: Arc<Context>,
        mail_cache_path: PathBuf,
        mail_cache_size: u64,
    ) -> Result<Arc<Self>, MailContextError> {
        Ok(Arc::new(Self {
            core_context,
            mail_cache_path,
            attachment_cache_size: mail_cache_size,
            active_user_contexts: Mutex::new(HashMap::new()),
            http_client: OnceLock::new(),
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
    pub async fn new_login_flow(&self) -> CoreContextResult<LoginFlow> {
        let _ = self.core_context.get_encryption_key()?;
        let session = self.core_context.new_api_session(None).await?;
        let device_info = self
            .core_context
            .get_service::<DeviceInfoService>()
            .get_device_info()
            .await;

        let challenge_info = ChallengeInfo {
            product_name: self.core_context.get_client_id().to_owned(),
            device_info,
            // Behaviours will be populated during the login flow (if available)
            recovery_behavior: None,
            username_behavior: None,
        };

        let migration_snooper = Box::new(MailMigrationSnooper::new(Arc::clone(&self.core_context)));

        let post_login_validator = Box::new(DefaultPostLoginValidator::new(
            Some(MAIL_ALLOWED_FREE_USER_COUNT),
            Arc::clone(&self.core_context),
        ));

        Ok(LoginFlow::new(
            session,
            challenge_info,
            migration_snooper,
            post_login_validator,
        ))
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
    ) -> MailContextResult<LoginFlow> {
        let key = self.core_context.get_encryption_key()?;

        let tether = self.core_context.account_stash().connection();

        let account = CoreAccount::find_by_id(user_id.clone(), &tether)
            .await?
            .ok_or(MailContextError::Other(anyhow!("account not found")))?;

        let session = CoreSession::find_by_id(session_id.clone(), &tether)
            .await?
            .ok_or(MailContextError::Other(anyhow!("session not found")))?;

        let api_session = self.core_context.new_api_session(Some(&session)).await?;

        let migration_snooper = Box::new(MailMigrationSnooper::new(Arc::clone(&self.core_context)));

        let post_login_validator = Box::new(DefaultPostLoginValidator::new(
            Some(MAIL_ALLOWED_FREE_USER_COUNT),
            Arc::clone(&self.core_context),
        ));

        match self
            .core_context
            .get_account_state(user_id.clone())
            .await?
            .ok_or(MailContextError::AccountMissing(user_id.clone()))?
        {
            CoreAccountState::NotReady => {
                Err(MailContextError::Other(anyhow!("account not ready")))
            }

            CoreAccountState::LoggedIn(_) => Err(MailContextError::Other(anyhow!(
                "account already logged in"
            ))),

            CoreAccountState::NeedMbp(_) => Ok(LoginFlow::new_from_mbp(
                api_session,
                user_id,
                session_id,
                migration_snooper,
                post_login_validator,
            )),

            CoreAccountState::NeedTfa(_) => {
                let password = (account.password)
                    .map(|p| p.decrypt_to_string(&key))
                    .transpose()
                    .or(Err(CoreContextError::Crypto))?
                    .map(|p| p.expose_secret().to_owned())
                    .ok_or(MailContextError::Other(anyhow!("password not found")))?;

                Ok(LoginFlow::new_from_tfa(
                    api_session,
                    user_id,
                    session_id,
                    password,
                    None, // Don't use persisted FIDO2 details - they are single-use
                    migration_snooper,
                    post_login_validator,
                ))
            }

            CoreAccountState::NeedNewPass(_) => Ok(LoginFlow::new_from_new_password(
                api_session,
                user_id,
                session_id,
                migration_snooper,
                post_login_validator,
            )),

            CoreAccountState::LoggedOut => {
                Err(MailContextError::Other(anyhow!("account logged out")))
            }
        }
    }

    /// Begin a signup flow.
    ///
    /// # Errors
    ///
    /// See [`Context::new_signup_flow`].
    pub async fn new_signup_flow(&self) -> Result<SignupFlow, CoreContextError> {
        // Ensure we have an encryption key
        let _ = self.core_context.get_encryption_key()?;

        // Create a new API session
        let session = self.core_context.new_api_session(None).await?;
        let (client, SessionParts { store, .. }) = session.into_parts();

        // Obtain device info (if possible)
        let device_info = self
            .core_context
            .get_service::<DeviceInfoService>()
            .get_device_info()
            .await;

        // Build challenge info
        let challenge_info = ChallengeInfo {
            product_name: self.core_context.get_client_id().to_owned(),
            device_info,
            // Behaviours will be populated during the sign up flow (if available)
            recovery_behavior: None,
            username_behavior: None,
        };

        let post_login_validator = Box::new(DefaultPostLoginValidator::new(
            Some(MAIL_ALLOWED_FREE_USER_COUNT),
            Arc::clone(&self.core_context),
        ));

        // Create a new signup flow
        Ok(
            SignupFlow::new(client, store, challenge_info, post_login_validator)
                .await
                .map_err(|api_err| anyhow!(api_err.to_string()))?,
        )
    }

    pub async fn verify_pin_code(self: &Arc<Self>, pin: Vec<u32>) -> MailContextResult<()> {
        self.handle_pin_code_action(pin, PinCode::verify).await
    }

    pub async fn delete_pin_code(self: &Arc<Self>, pin: Vec<u32>) -> MailContextResult<()> {
        self.handle_pin_code_action(pin, PinCode::delete).await
    }

    async fn handle_pin_code_action<F>(
        self: &Arc<Self>,
        pin: Vec<u32>,
        action: impl FnOnce(Arc<Context>, Vec<u32>) -> F,
    ) -> MailContextResult<()>
    where
        F: Future<Output = Result<(), PinError>>,
    {
        let ctx = self.core_context.clone();

        match action(ctx, pin).await {
            Err(PinError::TooManyAttempts) => {
                let mut user_ctxs = self.get_all_logged_in_user_ctx().await?;
                if let Some(ctx) = user_ctxs.pop() {
                    ctx.sign_out_all().await?;
                }
                Err(MailContextError::Pin(PinError::TooManyAttempts))
            }
            otherwise => Ok(otherwise?),
        }
    }

    /// Create a new context from a login flow.
    ///
    /// # Errors
    /// Returns error if the flow is in an invalid state or there was an issue initializing
    /// the user database.
    #[tracing::instrument(skip_all)]
    pub async fn user_context_from_login_flow(
        self: &Arc<Self>,
        flow: &mut LoginFlow,
    ) -> MailContextResult<Arc<MailUserContext>> {
        if !flow.is_logged_in() {
            return Err(MailContextError::Other(anyhow!("invalid login state")));
        }

        let user_id: UserId = flow
            .user_id()
            .map_err(|_| MailContextError::Other(anyhow!("invalid login state")))?
            .to_owned();
        let session_id: SessionId = flow
            .session_id()
            .map_err(|_| MailContextError::Other(anyhow!("invalid login state")))?
            .to_owned();
        let session = flow
            .take_session()
            .map_err(|_| MailContextError::Other(anyhow!("invalid login state")))?;

        let user_ctx = self
            .core_context
            .new_user_context(user_id, session, session_id)
            .await
            .map_err(MailContextError::from)?;
        Arc::clone(self)
            .new_user_context(user_ctx, ShouldInitializeMailUserContext::Yes)
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
    ) -> MailContextResult<Option<Arc<MailUserContext>>> {
        let ctx = self.core_context.user_context_from_session(session).await?;

        self.new_initialized_user_context(ctx).await
    }

    /// Create a new context from an existing session.
    ///
    /// # Errors
    /// Returns error if we failed to decrypt the user session or access the user database.
    pub async fn user_context_from_session(
        self: &Arc<Self>,
        session: &CoreSession,
        init: ShouldInitializeMailUserContext,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self.core_context.user_context_from_session(session).await?;

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
        let sessions = self.get_authenticated_sessions().await?;
        let mut ctxs = Vec::new();

        for session in sessions {
            ctxs.push(
                self.user_context_from_session(&session, ShouldInitializeMailUserContext::No)
                    .await?,
            );
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
        let sessions = self.get_authenticated_sessions().await?;
        let mut ctxs = Vec::new();

        for session in sessions.filter(|s| &s.remote_id != current_session_id) {
            ctxs.push(
                self.user_context_from_session(&session, ShouldInitializeMailUserContext::No)
                    .await?,
            );
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

    /// Get all authenticated API sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to retrieve the sessions from the db.
    pub async fn get_authenticated_sessions(
        &self,
    ) -> MailContextResult<impl Iterator<Item = CoreSession>> {
        Ok(self.core_context.get_authenticated_sessions().await?)
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
        tracing::info!("Logout account `{user_id}`");
        self.active_user_contexts.lock().await.remove(&user_id);
        Ok(self.core_context.logout_account(user_id).await?)
    }

    /// Logs out all sessions of an account and deletes the account's data.
    ///
    /// Unlike [`delete_account()`] the account metadata is preserved and is still
    /// listable in the session picker.
    ///
    /// Returns an error if the database operation fails.
    pub async fn logout_account_and_delete_user_data(
        &self,
        user_id: UserId,
    ) -> MailContextResult<()> {
        tracing::info!("Logout account `{user_id}`");
        self.active_user_contexts.lock().await.remove(&user_id);
        let mail_cache_path = self.mail_cache_path_for(&user_id);
        Ok(self
            .core_context
            .logout_and_delete_user_data(user_id, vec![mail_cache_path])
            .await?)
    }

    /// Removes a user session and deletes all associated data.
    ///
    /// This will also remove the user from the session picker.
    /// Use [`logout_account_and_delete_user_data()`] to preserve this entry.
    ///
    /// # Errors
    ///
    /// Returns error if the db operation failed. Though it will remove all user data
    /// first, which is non failing operations.
    ///
    pub async fn delete_account(&self, user_id: UserId) -> MailContextResult<()> {
        tracing::info!("Delete account `{user_id}`");
        self.active_user_contexts.lock().await.remove(&user_id);
        let mail_cache_path = self.mail_cache_path_for(&user_id);

        Ok(self
            .core_context
            .delete_account(user_id, vec![mail_cache_path])
            .await?)
    }

    #[must_use]
    pub fn mail_cache_path(&self) -> &Path {
        &self.mail_cache_path
    }

    #[must_use]
    pub fn mail_cache_path_for(&self, user_id: &UserId) -> PathBuf {
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

        if let Some(existing) = active_contexts.get(core_context.user_id())
            && let Some(upgraded) = existing.upgrade()
        {
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

        let ctx = MailUserContext::new(self.clone(), core_context).await?;

        active_contexts.insert(ctx.user_id().clone(), Arc::downgrade(&ctx));

        if matches!(init, ShouldInitializeMailUserContext::Yes) {
            MailUserContext::initialize_async(ctx.clone()).await?;
        }

        Ok(ctx)
    }

    /// See [`Context::spawn()`].
    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.core_context.spawn(task)
    }

    /// Get all the logged in user context that are active and initialized.
    pub async fn get_all_logged_in_and_initialized_user_contexts(
        self: &Arc<Self>,
    ) -> MailContextResult<Vec<Arc<MailUserContext>>> {
        let sessions = self.get_authenticated_sessions().await?;
        let mut ctxs = Vec::new();

        for session in sessions {
            match self.initialized_user_context_from_session(&session).await {
                Ok(Some(user_context)) => ctxs.push(user_context),
                Ok(None) => {
                    tracing::debug!("{} has non-initialized context", session.account_id);
                }
                Err(MailContextError::DuplicateContext(user_id)) => {
                    tracing::warn!("Duplicate context detected for {user_id}, skipping");
                }
                Err(e) => return Err(e),
            }
        }

        Ok(ctxs)
    }

    pub fn http_client(&self) -> &reqwest::Client {
        self.http_client.get_or_init(reqwest::Client::new)
    }

    pub fn network_monitor_service(&self) -> &NetworkMonitorService {
        self.core_context.network_monitor_service()
    }
}

impl Spawner for MailContext {
    fn spawn_task<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.spawn(f)
    }
}

pub struct MailUserDatabaseInitializer {}

#[async_trait::async_trait]
impl UserDatabaseInitializer for MailUserDatabaseInitializer {
    async fn initialize(&self, stash: &Stash) -> Result<(), MigratorError> {
        crate::db::migrations::migrate_db(stash).await?;
        Ok(())
    }
}
