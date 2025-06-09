mod action_queue;
mod events;
mod images;
mod initialization;

use crate::actions::draft::SEND_ACTION_GROUP;
use crate::actions::register_mail_actions;
use crate::draft::attachments::DraftStagingAreaCleaner;
use crate::models::{Conversation, Message};
use crate::prefetch::{Prefetch, PrefetchJob, PrefetchNotify};
use crate::user_context::initialization::InitializationMediator;
use crate::{AppError, MailContext, MailContextError, MailContextResult};
use anyhow::anyhow;
use proton_action_queue::queue::{Queue, QueueAutoExecutor, QueueAutoExecutorPool};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::crypto_clock;
use proton_core_api::services::proton::{AddressId, SessionId, UserId};
use proton_core_api::services::proton::{Proton, ProtonCore};
use proton_core_api::session::{CoreSession, Session};
use proton_core_common::datatypes::{AccountDetails, LocalAddressId};
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::{Address, User, UserSettings};
use proton_core_common::{ContactError, Context as CoreContext, CoreContextError, UserContext};
use proton_crypto_inbox::keys::{ComposerPreference, CryptoMailSettings, SendPreferences};
use proton_crypto_inbox::proton_crypto::CryptoClockProvider;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::{UnlockedAddressKeys, UnlockedUserKeys};
use proton_event_loop::EventPoll;
use proton_task_service::{AsyncTaskResult, TaskService, TaskSpawner};
use stash::orm::Model;
use stash::stash::{RunTransaction, Stash, Tether};
use std::future::Future;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;
use tokio::join;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::error;

const DEFAULT_SEND_QUEUE_POOL_SIZE: usize = 4;
const DEFAULT_PREFETCH_BOUND: usize = 4;

pub struct MailUserContext {
    this: Weak<Self>,
    mail_context: Arc<MailContext>,
    user_context: Arc<UserContext>,
    default_queue_executor: QueueAutoExecutor,
    send_queue_executors: QueueAutoExecutorPool,
    prefetch: PrefetchNotify,
    initialization_mediator: InitializationMediator,
    pub is_cleanup_cache_running: Arc<AtomicBool>,
}

impl MailUserContext {
    /// Create a new user context.
    pub(crate) async fn new(
        mail_context: Arc<MailContext>,
        user_context: Arc<UserContext>,
    ) -> MailContextResult<Arc<Self>> {
        register_mail_actions(user_context.queue());

        let online = user_context
            .session()
            .status_watcher()
            .subscribe_to_online();

        let task_service = mail_context.core_context().task_service().task_service();

        let default_queue_executor =
            Self::new_default_queue_executor(user_context.queue(), online.clone(), task_service);

        let send_queue_executors = Self::new_send_queue_executor(
            user_context.queue(),
            online,
            NonZeroUsize::new(DEFAULT_SEND_QUEUE_POOL_SIZE).unwrap(),
            task_service,
        );

        let initialization_mediator = InitializationMediator::new(task_service);

        let this = Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            mail_context,
            user_context,
            prefetch: OnceLock::new(),
            default_queue_executor,
            send_queue_executors,
            initialization_mediator,
            is_cleanup_cache_running: Default::default(),
        });

        // Start draft staging area cleaner.
        DraftStagingAreaCleaner::new().run(Arc::clone(&this))?;

        this.user_context
            .queue()
            .register_execution_context(Weak::clone(&this.this));
        this.init_expiration_loop();
        this.register_subscribers().await?;

        if let EventPollMode::Automatic(interval) = this.user_context().event_poll_mode() {
            this.init_event_loop_poll(interval)
                .await
                .inspect_err(|e| error!("Failed to init event loop task: {e:?}"))?;
        }

        Ok(this)
    }

    /// Create a new default action executor.
    fn new_default_queue_executor(
        queue: &Queue,
        online: watch::Receiver<bool>,
        task_service: &TaskService,
    ) -> QueueAutoExecutor {
        queue
            .new_executor()
            .into_auto_executor(online, task_service)
    }

    /// Create a new send group action executor.
    fn new_send_queue_executor(
        queue: &Queue,
        online: watch::Receiver<bool>,
        pool_size: NonZeroUsize,
        task_service: &TaskService,
    ) -> QueueAutoExecutorPool {
        QueueAutoExecutorPool::new(queue, &SEND_ACTION_GROUP, pool_size, online, task_service)
    }

    /// Get the current Arc instance for this context.
    #[must_use]
    pub fn as_arc(&self) -> Arc<Self> {
        self.this.upgrade().expect("Should never fail")
    }

    /// Get a weak reference to this context.
    #[must_use]
    pub fn as_weak(&self) -> Weak<Self> {
        Weak::clone(&self.this)
    }

    /// Sets a background job where every 60 seconds it deletes all of the messages and conversations
    /// that have an expiration date.
    fn init_expiration_loop(&self) {
        let ctx = self.this.clone();
        self.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                let Some(ctx) = ctx.upgrade() else {
                    return;
                };
                let mut tether = ctx.user_stash().connection();
                if let Err(e) = Conversation::delete_expired(&mut tether).await {
                    error!("Error in background task deleting expired conversations: {e:?}");
                }

                if let Err(e) = Message::delete_expired(&mut tether).await {
                    error!("Error in background task deleting expired messages: {e:?}");
                }
                drop(tether);
                drop(ctx);
                interval.tick().await;
            }
        });
    }

    pub fn session(&self) -> &Session {
        self.user_context.session()
    }

    /// Get the action queue instance.
    pub fn action_queue(&self) -> &Queue {
        self.user_context.queue()
    }

    /// Pause all action queue executors.
    pub fn pause_queue_executors(&self) {
        self.default_queue_executor.pause();
        self.send_queue_executors.pause();
    }

    /// Unpause all action queue executors.
    pub fn unpause_queue_executors(&self) {
        self.default_queue_executor.unpause();
        self.send_queue_executors.unpause();
    }

    /// Terminate all action queue executors.
    pub fn terminate_queue_executors(&self) {
        self.default_queue_executor.terminate();
        self.send_queue_executors.terminate();
    }

    /// Get the API service.
    pub fn api(&self) -> &Proton {
        self.user_context.session().api()
    }

    /// Get the database connection.
    #[must_use]
    pub fn user_stash(&self) -> &Stash {
        self.user_context.stash()
    }

    /// Get the event loop.
    #[must_use]
    pub fn event_loop(&self) -> &EventPoll {
        self.user_context.event_loop()
    }

    /// Get the mail context within which this user context resides.
    pub fn mail_context(&self) -> &MailContext {
        &self.mail_context
    }

    /// Get the mail context within which this user context resides.
    pub fn mail_context_arc(&self) -> &Arc<MailContext> {
        &self.mail_context
    }

    /// Get the core context within which this user context resides.
    pub fn core_context(&self) -> &Arc<CoreContext> {
        self.mail_context.core_context()
    }

    /// Get the inner core context which this context wraps.
    pub fn user_context(&self) -> &UserContext {
        &self.user_context
    }

    /// Get the inner core context which this context wraps as an Arc.
    pub fn user_context_arc(&self) -> &Arc<UserContext> {
        &self.user_context
    }

    /// Get `MailUserContext` for each logged in account.
    ///
    pub async fn all_mail_user_ctxs(&self) -> MailContextResult<Vec<Arc<Self>>> {
        self.mail_context.get_all_logged_in_user_ctx().await
    }

    /// Get `MailUserContext` for any other than self, logged in account.
    ///
    pub async fn other_mail_user_ctxs(&self) -> MailContextResult<Vec<Arc<Self>>> {
        self.mail_context
            .get_other_logged_in_user_ctx(self.session_id())
            .await
    }

    /// Get the remote (API) ID of the user associated with this context.
    pub fn user_id(&self) -> &UserId {
        self.user_context.user_id()
    }

    /// Get the remote (API) ID of the session associated with this context.
    pub fn session_id(&self) -> &SessionId {
        self.user_context.session_id()
    }

    /// Provides a way to get the core::models::User instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> MailContextResult<User> {
        let stash = self.user_stash();
        let tether = stash.connection();
        let user_id = self.user_id();
        let real_user = User::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| MailContextError::Other(anyhow!("Missing User, this is a bug.")))?;

        Ok(real_user)
    }

    /// Retrieves the account details of the current account.
    ///
    /// Returns the active account's details or an error if active account does not exist.
    ///
    /// # Errors
    /// - Returns `MailContextError::Other` if the active account is missing.
    pub async fn account_details(&self) -> MailContextResult<AccountDetails> {
        let account_details = self.user_context.account_details().await?;
        Ok(account_details)
    }

    /// Retrieves the user's settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn user_settings(&self) -> MailContextResult<UserSettings> {
        Ok(self.user_context.user_settings().await?)
    }

    /// Returns the unlocked user keys of this user.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `conn`         - The database connection to load the keys from database.
    ///
    /// # Errors
    /// Returns a wrapped [`MailContextError::KeyHandlingError`] if the operation fails.
    ///
    pub async fn unlocked_user_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        conn: &Tether,
    ) -> MailContextResult<UnlockedUserKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_user_keys(pgp_provider, conn, self.session())
            .await?;
        Ok(keys)
    }

    /// Returns the unlocked address keys of this user for the provided address.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `conn`         - The database connection to load the keys from database.
    /// * `address_id`   - The address identifier to load the keys for.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    ///
    pub async fn unlocked_address_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        conn: &Tether,
        address_id: &AddressId,
    ) -> MailContextResult<UnlockedAddressKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_address_keys(pgp_provider, conn, self.session(), address_id)
            .await?;
        Ok(keys)
    }

    /// Loads the send preferences of the recipient with the given email address.
    ///
    /// [`SendPreferences`] contains the send preferences for sending an email to the given recipient
    /// including encryption/signing/formatting options and the encryption key.
    /// The send preferences are used to build the request for sending emails via Proton.
    /// [internal confluence docs](https://protonag.atlassian.net/wiki/spaces/MAILFE/pages/53117391/Send+preferences+for+outgoing+email)
    /// This information is collected from the keys returned by the API, contact vCard data,
    /// sender mail settings, and composer preferences.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider`        - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `tx `                 - The transaction to query from.
    /// * `email`               - The email address of the recipient.
    /// * `settings`            - The [`CryptoMailSettings`] extracted from the mail settings [`super::models::MailSettings::crypto_mail_settings`]
    /// * `composer_preference` - (currently unused) The composer preferences, use [`ComposerPreference::default()`].
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] or [`proton_crypto_inbox::keys::EncryptionPreferencesError`] if the operation fails.
    ///
    pub async fn recipient_send_preferences<Provider>(
        &self,
        pgp_provider: &Provider,
        rt: &mut impl RunTransaction,
        email: &str,
        settings: CryptoMailSettings,
        composer_preference: ComposerPreference,
    ) -> MailContextResult<SendPreferences<Provider::PublicKey>>
    where
        Provider: PGPProviderSync,
    {
        let encryption_time = crypto_clock::server_crypto_clock().unix_time();

        // If the email is from an owned address by the user, use the corresponding keys.
        if let Some(address) = Address::by_email(email, rt.tether())
            .await
            .inspect_err(|err| {
                error!("send preferences: failed to search address by email: {err:?}")
            })?
        {
            let address_rid = address.remote_id.as_ref().ok_or_else(|| {
                MailContextError::App(AppError::AddressHasNoRemoteId(
                    address.local_id.unwrap_or(LocalAddressId::from(0)),
                ))
            })?;

            let address_keys = self
                .unlocked_address_keys(pgp_provider, rt.tether(), address_rid)
                .await
                .inspect_err(|err| error!("send preferences for self: {err:?}"))?;
            let send_preferences =
                SendPreferences::new_for_self(&address_keys, encryption_time, settings)
                    .inspect_err(|err| error!("send preferences for self: {err:?}"))?;
            return Ok(send_preferences);
        }

        let user_keys = self.unlocked_user_keys(pgp_provider, rt.tether()).await?;
        // Fetch API keys, and contact-pinned keys concurrently.
        let (api_keys_result, vcard_keys_result) = join!(
            self.user_context
                .public_address_keys(pgp_provider, email, false),
            self.user_context.public_address_keys_from_contacts(
                pgp_provider,
                rt,
                &user_keys,
                email
            )
        );

        // Handle error when loading contact keys, but ignore CardNotFound as it's valid to have no contact.
        if let Err(e) = &vcard_keys_result {
            if !matches!(
                e,
                CoreContextError::ContactError(ContactError::CardNotFound(_))
            ) {
                error!(
                    "send preferences: failed to load contact pinned keys: {}",
                    e
                );
            }
        }

        // On error, we currently assume no pinned keys exists.
        let vcard_keys = vcard_keys_result.ok().flatten();

        let send_preferences = SendPreferences::new(
            api_keys_result?,
            vcard_keys,
            encryption_time,
            &settings,
            composer_preference,
        )
        .inspect_err(|err| error!("send preferences: {err:?}"))?;

        Ok(send_preferences)
    }

    /// Logs this user out.
    ///
    pub async fn logout(&self) -> MailContextResult<()> {
        self.mail_context
            .logout_account(self.user_id().to_owned())
            .await?;

        Ok(())
    }

    /// Unlike [sign_out()] the user's metadata is preserved so it still shows up in the session
    /// picker.
    pub async fn logout_and_delete_data(&self) -> MailContextResult<()> {
        self.mail_context
            .logout_account_and_delete_user_data(self.user_id().clone())
            .await?;

        Ok(())
    }

    /// Sign this user out.
    ///
    /// Method will delete user's account and data from the device. The user will no longer
    /// be available in the session picker.
    ///
    pub async fn sign_out(&self) -> MailContextResult<()> {
        self.mail_context
            .delete_account(self.user_id().clone())
            .await?;

        Ok(())
    }

    /// Sign out from all accounts.
    ///
    /// This method will perform:
    /// * Each user logout
    /// * Clear all user data
    /// * Clear keychains
    /// * Remove all account data & caches
    ///
    /// There are sevral layers to this function in which most of them
    /// are non failing and retrying in cases where we could fail.
    ///
    /// ### Errors
    ///
    /// The method may fail to gather users. This will happen if for any
    /// reason we will be unable to read sessions from account database.
    ///
    /// ### Notes
    ///
    /// There are no guarantees to clear everything especially on
    /// operating systems which locks files (looking at you Windows)
    /// though it will make best effort to get rid of any information
    /// app has stored over the course of its life.
    ///
    pub async fn sign_out_all(&self) -> MailContextResult<()> {
        let all_ctxs = self.all_mail_user_ctxs().await?;

        for ctx in all_ctxs {
            // If for any reason we fail to sign out account it will
            // be brought down anyway by tear_down in the next step
            // which also will get rid of key which is essential to
            // read data from API
            let _ = ctx
                .sign_out()
                .await
                .inspect_err(|e| tracing::error!("Could not remove account, `{e}`"));
        }

        self.mail_context().core_context().tear_down().await;

        Ok(())
    }

    /// Ping the proton servers to see if they are responsive/alive.
    pub async fn ping(&self) -> MailContextResult<()> {
        self.user_context
            .session()
            .api()
            .get_tests_ping(None, None)
            .await?;
        Ok(())
    }

    /// Get the connection status of the current user session.
    pub async fn connection_status(&self) -> ConnectionStatus {
        self.user_context.connection_status().await
    }

    /// Prefetch key locations in the background.
    ///
    /// Following priority locations are prefetched:
    /// - Inbox
    /// - Sent
    /// - AllSent
    /// - Drafts
    /// - AllDrafts
    pub async fn prefetch(self: &Arc<Self>) -> MailContextResult<()> {
        let tether = self.user_stash().connection();
        let locations = PrefetchJob::default_locations(&tether).await;

        self.queue_prefetch_jobs(locations).await
    }

    pub async fn queue_prefetch_jobs(
        self: &Arc<Self>,
        jobs: Vec<PrefetchJob>,
    ) -> MailContextResult<()> {
        if let Some(sender) = self.prefetch.get() {
            sender.send_async(jobs).await.map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send prefetch signal to prefetcher"))
            })?;

            Ok(())
        } else {
            let (sender, receiver) = flume::bounded(DEFAULT_PREFETCH_BOUND);

            self.prefetch.set(sender).map_err(|e| {
                MailContextError::Other(anyhow!("Failed to set prefetch sender: {e:?}"))
            })?;
            Prefetch::initialize(self.clone(), receiver).await;
            // unwrap safety: `self.prefetch` is set just above, it cannot be `None`.
            self.prefetch
                .get()
                .unwrap()
                .send_async(jobs)
                .await
                .map_err(|_| {
                    MailContextError::Other(anyhow!("Failed to send prefetch signal to prefetcher"))
                })?;

            Ok(())
        }
    }

    /// Get the path to the attachment staging folder.
    ///
    /// Attachment staging is used by mobile to place attachment files so they can
    /// be consumed later by the SDK. We can't directly use file system paths
    pub fn attachment_staging_path(&self) -> PathBuf {
        self.mail_context
            .mail_cache_path(self.user_id())
            .join("attachment-staging")
    }

    /// See [`UserContext::spawn()`].
    pub fn spawn<F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.user_context.spawn(task)
    }

    /// See [`UserContext::spawn_with()`].
    pub fn spawn_with<S, F>(&self, task: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        S: TaskSpawner,
        F: Future<Output: Send> + Send + 'static,
    {
        self.user_context.spawn_with::<S, _>(task)
    }
}
