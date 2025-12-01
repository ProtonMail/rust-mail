#![allow(clippy::result_large_err)]

mod action_queue;
mod attachment_cache;
mod builder;
mod events;
mod images;
mod initialization;

use crate::actions::PREFETCH_ROLLBACK_ACTION_GROUP;
use crate::actions::draft::{SEND_ACTION_GROUP, SHARE_EXT_ACTION_GROUP};
use crate::db::online_migrations;
use crate::draft::attachments::DraftStagingAreaCleaner;
use crate::events::MailEvent;
use crate::models::{Conversation, Message};
#[cfg(feature = "prefetch")]
use crate::prefetch::{Prefetch, PrefetchJob, PrefetchService};
use crate::rsvp::RsvpService;
use crate::upsell_eligibility_watcher::UpsellEligibilityWatcher;
use crate::{AppError, ImageLoader, MailContext, MailContextError, MailContextResult};
use anyhow::anyhow;
use attachment_cache::AttachmentCacheState;
use builder::MailUserContextBuilder;
use events::subscriber::MailEventSubscriber;
use initialization::InitializationMediator;
use proton_account_api::password::PasswordFlow;
use proton_action_queue::action::ActionGroup;
use proton_action_queue::queue::{Queue, QueueAutoExecutorPool};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::crypto_clock;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::services::proton::{AddressId, PrivateEmailRef, SessionId, UserId};
use proton_core_api::session::Session;
use proton_core_common::datatypes::{
    AccountDetails, AddressStatus, BlackFridayWave, LocalAddressId, NotificationSettings,
    UpsellEligibility, UpsellType,
};
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::{Address, PaidSubscription, Role, User, UserSettings};
use proton_core_common::services::{
    EventPollConfigService, NetworkMonitorService, UserIssueReporterService,
};
use proton_core_common::{
    ContactError, Context as CoreContext, CoreContextError, KeyHandlingError, Origin, UserContext,
    services::UserMetricService,
};
use proton_crypto_inbox::keys::{ComposerPreference, CryptoMailSettings, SendPreferences};
use proton_crypto_inbox::proton_crypto::CryptoClockProvider;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::{UnlockedAddressKeys, UnlockedUserKeys};
use proton_event_loop::Subscriber;
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use proton_task_service::Spawner;
use stash::orm::Model;
use stash::stash::{RunTransaction, Stash, StashError, Tether, WatcherHandle};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::join;
use tokio::task::JoinHandle;
use tracing::{debug, error, instrument};

const DEFAULT_SEND_QUEUE_POOL_SIZE: usize = 4;
const DEFAULT_DEFAULT_QUEUE_POOL_SIZE: usize = 1;

const DEFAULT_PREFETCH_ROLLBACK_QUEUE_POOL_SIZE: usize = 1;
const DEFAULT_SHARE_EXT_QUEUE_POOL_SIZE: usize = 2;

#[cfg(feature = "prefetch")]
const DEFAULT_PREFETCH_BOUND: usize = 10;

// Feature flags
const FF_BLACK_FRIDAY: &str = "MailBlackFriday2025";
const FF_BLACK_FRIDAY_WAVE2: &str = "MailBlackFriday2025Wave2";

/// App origin only
pub struct DefaultQueueExecutor {
    pub default: QueueAutoExecutorPool,
    pub prefetch_rollback: QueueAutoExecutorPool,
}

impl DefaultQueueExecutor {
    pub fn pause(&self) {
        self.default.pause();
        self.prefetch_rollback.pause();
    }

    pub fn pause_prefetch_rollback(&self) {
        self.prefetch_rollback.pause();
    }

    pub fn resume(&self) {
        self.default.resume();
        self.prefetch_rollback.resume();
    }

    pub fn resume_prefetch_rollback(&self) {
        self.prefetch_rollback.resume();
    }

    pub fn terminate(&self) {
        self.default.terminate();
        self.prefetch_rollback.terminate();
    }
}

/// Used by both App and ShareExt origins
pub struct SendQueueExecutorPool {
    pub pool: QueueAutoExecutorPool,
}

impl SendQueueExecutorPool {
    pub fn pause(&self) {
        self.pool.pause();
    }

    pub fn resume(&self) {
        self.pool.resume();
    }

    pub fn terminate(&self) {
        self.pool.terminate();
    }
}

pub struct QueuesService {
    weak: Weak<MailUserContext>,
}

impl QueuesService {
    pub fn new(weak: Weak<MailUserContext>) -> Self {
        Self { weak }
    }

    #[tracing::instrument(skip_all)]
    pub fn pause(&self) {
        let Some(ctx) = self.weak.upgrade() else {
            tracing::error!("Could not upgrade weak ctx reference");
            return;
        };
        if let Some(service) = ctx.get_service_opt::<DefaultQueueExecutor>() {
            service.pause();
        };
        ctx.get_service::<SendQueueExecutorPool>().pause();
    }

    #[tracing::instrument(skip_all)]
    pub fn resume(&self) {
        let Some(ctx) = self.weak.upgrade() else {
            tracing::error!("Could not upgrade weak ctx reference");
            return;
        };
        if let Some(service) = ctx.get_service_opt::<DefaultQueueExecutor>() {
            service.resume();
        };
        ctx.get_service::<SendQueueExecutorPool>().resume();
    }

    #[tracing::instrument(skip_all)]
    pub fn terminate(&self) {
        let Some(ctx) = self.weak.upgrade() else {
            tracing::error!("Could not upgrade weak ctx reference");
            return;
        };
        if let Some(service) = ctx.get_service_opt::<DefaultQueueExecutor>() {
            service.terminate();
        };
        ctx.get_service::<SendQueueExecutorPool>().terminate();
    }
}

pub struct MailUserContext {
    this: Weak<Self>,
    mail_context: Arc<MailContext>,
    user_context: Arc<UserContext>,

    services: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl MailUserContext {
    #[instrument(name = "NewMailUserContext", skip_all, fields(mode, user_id=%user_context.user_id()))]
    pub(crate) async fn new(
        mail_context: Arc<MailContext>,
        user_context: Arc<UserContext>,
    ) -> MailContextResult<Arc<Self>> {
        tracing::info!("Creating MailUserContext");
        let user_context_cloned = user_context.clone();

        async {
            let span =
                tracing::debug_span!(parent: None, "qac", user_id = %user_context.user_id().short_id());

            let origin = mail_context.core_context().origin();

            let mut builder =
                MailUserContextBuilder::new()
                    .with_service(AttachmentCacheState::new())
                    .with_cyclic_service(ImageLoader::new);

            builder = match origin {
                Origin::App => {
                    let builder = builder
                        .with_service(SendQueueExecutorPool {
                            pool: QueueAutoExecutorPool::new(
                                user_context.queue(),
                                &SEND_ACTION_GROUP,
                                NonZeroUsize::new(DEFAULT_SEND_QUEUE_POOL_SIZE).unwrap(),
                                mail_context.core_context().as_ref(),
                                true,
                                user_context.as_ref(),
                                span.clone(),
                            ),
                        })
                        .with_service(DefaultQueueExecutor {
                            default: QueueAutoExecutorPool::new(
                                user_context.queue(),
                                &ActionGroup::default(),
                                NonZeroUsize::new(DEFAULT_DEFAULT_QUEUE_POOL_SIZE).unwrap(),
                                mail_context.core_context().as_ref(),
                                true,
                                user_context.as_ref(),
                                span.clone(),
                            ),
                            prefetch_rollback: QueueAutoExecutorPool::new(
                                user_context.queue(),
                                &PREFETCH_ROLLBACK_ACTION_GROUP,
                                NonZeroUsize::new(DEFAULT_PREFETCH_ROLLBACK_QUEUE_POOL_SIZE)
                                    .unwrap(),
                                mail_context.core_context().as_ref(),
                                true,
                                user_context.as_ref(),
                                span.clone(),
                            ),
                        })
                        .with_cyclic_service(QueuesService::new)
                        .with_service(InitializationMediator::new(
                            mail_context.core_context().task_service().task_service(),
                        ))
                        .with_service(RsvpService::new(user_context.stash()));

                    #[cfg(feature = "prefetch")]
                    let builder = { builder.with_service(PrefetchService::new()) };

                    builder
                }

                Origin::ShareExt => builder
                    .with_service(SendQueueExecutorPool {
                        pool: {
                            if let Err(e) = Queue::delete_all_in_group(
                                user_context.queue(),
                                SHARE_EXT_ACTION_GROUP.clone(),
                            )
                            .await
                            {
                                tracing::warn!("Could not clear share extension queue: {}", e);
                                tracing::warn!("Continuing with existing queue");
                            };
                            QueueAutoExecutorPool::new(
                                user_context.queue(),
                                &SHARE_EXT_ACTION_GROUP,
                                NonZeroUsize::new(DEFAULT_SHARE_EXT_QUEUE_POOL_SIZE).unwrap(),
                                mail_context.core_context().as_ref(),
                                true,
                                user_context.as_ref(),
                                span.clone(),
                            )
                        },
                    })
                    .with_cyclic_service(QueuesService::new),

            };

            let this = builder.build(mail_context, user_context).await?;

            // Catch invalid actions at this stage to interrupt the context creation
            // and avoid infinite error loops.
            if let Err(e) = this.user_context.queue().validate_queued_actions().await {
                return Err(MailContextError::NonProcessableActions(e));
            }

            match origin {
                Origin::App => {
                    DraftStagingAreaCleaner::new().run(&this)?;
                    this.init_expiration_loop();
                    this.register_subscribers().await?;
                    online_migrations::run(&this).await?;

                    let config = this
                        .mail_context()
                        .core_context()
                        .get_service::<EventPollConfigService>();

                    if let EventPollMode::Automatic(interval) = config.mode() {
                        this.init_event_loop_poll(interval)?;
                    }
                }

                Origin::ShareExt => {
                    //
                }
            }

            // There's a race condition between initializing queues and `self` - to
            // avoid it, we start our queues as paused and resume once everything
            // has been initialized, i.e. here:
            this.queues().resume();

            tracing::info!("Creating MailUserContext...Done");
            Ok(this)
        }
        .await
        .inspect_err(|e| {
            if !e.is_network_failure() {
                user_context_cloned.issue_reporter_service().report(
                    IssueLevel::Critical,
                    "Failed to create new mail user context".into(),
                    issue_report_keys_from_error(e),
                )
            }
        })
    }

    #[must_use]
    pub fn get_service<T: Any + Send + Sync + 'static>(&self) -> &T {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|service| service.downcast_ref::<T>())
            .unwrap_or_else(|| {
                panic!(
                    "Required service {} not found in context",
                    std::any::type_name::<T>()
                )
            })
    }

    pub fn get_service_opt<T: Any + Send + Sync + 'static>(&self) -> Option<&T> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|service| service.downcast_ref::<T>())
    }

    pub fn has_service<T: Any + Send + Sync + 'static>(&self) -> bool {
        self.services.contains_key(&TypeId::of::<T>())
    }

    pub fn queues(&self) -> &QueuesService {
        self.get_service::<QueuesService>()
    }

    #[must_use]
    pub fn as_arc(&self) -> Arc<Self> {
        self.this.upgrade().expect("Should never fail")
    }

    #[must_use]
    pub fn as_weak(&self) -> Weak<Self> {
        Weak::clone(&self.this)
    }

    #[must_use]
    pub fn origin(&self) -> Origin {
        self.core_context().origin()
    }

    /// Sets a background job where every 60 seconds it deletes all of the messages and conversations
    /// that have an expiration date.
    fn init_expiration_loop(&self) {
        let ctx = self.this.clone();
        self.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                let Some(ctx) = ctx.upgrade() else {
                    return;
                };
                if let Ok(mut tether) = ctx.user_stash().connection().await {
                    if let Err(e) = Conversation::delete_expired(&mut tether).await {
                        error!("Error in background task deleting expired conversations: {e:?}");
                    }

                    if let Err(e) = Message::delete_expired(&mut tether).await {
                        error!("Error in background task deleting expired messages: {e:?}");
                    }
                }
                drop(ctx);
                interval.tick().await;
            }
        });
    }

    pub fn session(&self) -> &Session {
        self.user_context.session()
    }

    pub fn action_queue(&self) -> &Queue {
        self.user_context.queue()
    }

    #[must_use]
    pub fn user_stash(&self) -> &Stash {
        self.user_context.stash()
    }

    pub fn mail_context(&self) -> &MailContext {
        &self.mail_context
    }

    pub fn mail_context_arc(&self) -> &Arc<MailContext> {
        &self.mail_context
    }

    pub fn core_context(&self) -> &Arc<CoreContext> {
        self.mail_context.core_context()
    }

    pub fn user_context(&self) -> &UserContext {
        &self.user_context
    }

    pub fn observability_service(&self) -> &UserMetricService {
        self.user_context.get_service::<UserMetricService>()
    }

    pub async fn all_mail_user_ctxs(&self) -> MailContextResult<Vec<Arc<Self>>> {
        self.mail_context.get_all_logged_in_user_ctx().await
    }

    pub async fn other_mail_user_ctxs(&self) -> MailContextResult<Vec<Arc<Self>>> {
        self.mail_context
            .get_other_logged_in_user_ctx(self.session_id())
            .await
    }

    pub fn user_id(&self) -> &UserId {
        self.user_context.user_id()
    }

    pub fn session_id(&self) -> &SessionId {
        self.user_context.session_id()
    }

    pub(crate) fn rsvp_service(&self) -> &RsvpService {
        self.get_service::<RsvpService>()
    }

    pub fn attachment_cache_state(&self) -> &AttachmentCacheState {
        self.get_service::<AttachmentCacheState>()
    }

    pub async fn user(&self) -> MailContextResult<User> {
        let stash = self.user_stash();
        let tether = stash.connection().await?;
        let user_id = self.user_id();
        let real_user = User::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| MailContextError::Other(anyhow!("Missing User, this is a bug.")))?;

        Ok(real_user)
    }

    pub async fn account_details(&self) -> MailContextResult<AccountDetails> {
        let account_details = self.user_context.account_details().await?;
        Ok(account_details)
    }

    pub async fn user_settings(&self) -> MailContextResult<UserSettings> {
        Ok(self.user_context.user_settings().await?)
    }

    pub async fn unlocked_user_keys<P>(
        &self,
        pgp: &P,
        conn: &Tether,
    ) -> MailContextResult<UnlockedUserKeys<P>>
    where
        P: PGPProviderSync,
    {
        let keys = self
            .user_context
            .unlocked_user_keys(pgp, conn, self.session())
            .await?;
        Ok(keys)
    }

    pub async fn unlocked_address_keys<P>(
        &self,
        pgp: &P,
        conn: &Tether,
        address_id: &AddressId,
    ) -> MailContextResult<UnlockedAddressKeys<P>>
    where
        P: PGPProviderSync,
    {
        let keys = self
            .user_context
            .unlocked_address_keys(pgp, conn, self.session(), address_id)
            .await?;
        Ok(keys)
    }

    pub async fn watch_upsell_eligibility(&self) -> Result<WatcherHandle, StashError> {
        UpsellEligibilityWatcher::watch(self.user_stash()).await
    }

    pub async fn upsell_eligibility(&self) -> MailContextResult<UpsellEligibility> {
        let user = self.user().await?;

        if user.subscribed != PaidSubscription::empty() || user.role == Role::Member {
            Ok(UpsellEligibility::NotEligible)
        } else {
            let upsell_type = self.upsell_type(user).await?;
            Ok(UpsellEligibility::Eligible(upsell_type))
        }
    }

    async fn upsell_type(&self, user: User) -> MailContextResult<UpsellType> {
        let feature_flags = self.user_context().feature_flags();
        let black_friday_promo_live = feature_flags
            .get(FF_BLACK_FRIDAY)
            .await?
            .unwrap_or_default();
        let black_friday_promo_wave2 = feature_flags
            .get(FF_BLACK_FRIDAY_WAVE2)
            .await?
            .unwrap_or_default();

        if black_friday_promo_live {
            let in_app_notifications_enabled = self
                .user_settings()
                .await?
                .news
                .contains(NotificationSettings::IN_APP_NOTIFICATIONS);

            if in_app_notifications_enabled && !user.is_deliquent() {
                let wave = if black_friday_promo_wave2 {
                    BlackFridayWave::Wave2
                } else {
                    BlackFridayWave::Wave1
                };
                return Ok(UpsellType::BlackFriday(wave));
            }
        }

        Ok(UpsellType::Standard)
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
    pub async fn recipient_send_preferences<P>(
        &self,
        pgp: &P,
        tx: &mut impl RunTransaction,
        email: PrivateEmailRef<'_>,
        settings: CryptoMailSettings,
        composer_preference: ComposerPreference,
    ) -> MailContextResult<SendPreferences<P::PublicKey>>
    where
        P: PGPProviderSync,
    {
        let encryption_time = crypto_clock::server_crypto_clock().unix_time();

        // If the email is from an owned address by the user and the address is active, use the corresponding keys.
        if let Some(address) = Address::by_email(email.as_clear_text_str(), tx.tether())
            .await
            .inspect_err(|err| {
                error!("send preferences: failed to search address by email: {err:?}")
            })?
        {
            if address.status == AddressStatus::Enabled {
                debug!("send preferences: loading from self-owned address");

                let address_rid = address.remote_id.as_ref().ok_or_else(|| {
                    MailContextError::App(AppError::AddressHasNoRemoteId(
                        address.local_id.unwrap_or(LocalAddressId::from(0)),
                    ))
                })?;

                let address_keys = self
                    .unlocked_address_keys(pgp, tx.tether(), address_rid)
                    .await
                    .inspect_err(|err| error!("send preferences for self: {err:?}"))?;

                let send_preferences = SendPreferences::new_for_self(
                    address.address_type.is_external(),
                    &address_keys,
                    encryption_time,
                    settings,
                    composer_preference,
                )
                .inspect_err(|err| error!("send preferences for self: {err:?}"))?;

                return Ok(send_preferences);
            }
        }

        debug!("send preferences: loading from contacts and key server");

        let user_keys = self.unlocked_user_keys(pgp, tx.tether()).await?;

        let email_cloned = email.clone();
        // Fetch API keys, and contact-pinned keys concurrently.
        let (api_keys_result, vcard_keys_result) = join!(
            self.user_context
                .public_address_keys(pgp, email_cloned, false),
            self.user_context
                .public_address_keys_from_contacts(pgp, tx, &user_keys, email)
        );

        // Handle error when loading contact keys, but ignore CardNotFound as it's valid to have no contact.
        if let Err(e) = &vcard_keys_result
            && !matches!(
                e,
                CoreContextError::ContactError(ContactError::CardNotFound(_))
            )
        {
            error!(
                "send preferences: failed to load contact pinned keys: {}",
                e
            );
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

    pub async fn new_password_change_flow(&self) -> MailContextResult<PasswordFlow> {
        let user = self.user().await?;
        let session = self.session().to_owned();
        let account = self.user_context.core_account().await?;
        let tfa_mode = account.second_factor_mode.unwrap_or_default();
        let mbp_mode = account.password_mode.unwrap_or_default();

        let key_secret = (session.expose_key_secret().await)
            .map(|s| s.expose_secret().to_owned())
            .ok_or(KeyHandlingError::NoUserSecret)
            .map_err(CoreContextError::PGPKeyAccess)?;

        Ok(PasswordFlow::new(
            session,
            user.email,
            user.keys.into(),
            key_secret,
            tfa_mode,
            mbp_mode,
        ))
    }

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
    ///
    /// There are sevral layers to this function in which most of them
    /// are non failing and retrying in cases where we could fail.
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

        Ok(())
    }

    pub async fn ping(&self) -> MailContextResult<()> {
        self.user_context
            .session()
            .get_tests_ping(None, None)
            .await?;
        Ok(())
    }

    pub fn connection_status(&self) -> ConnectionStatus {
        self.user_context.connection_status()
    }

    pub fn event_subscriber(&self) -> impl Subscriber<MailEvent> + 'static {
        MailEventSubscriber::new(Weak::clone(&self.this))
    }

    #[cfg(feature = "prefetch")]
    pub async fn queue_prefetch_jobs(
        self: &Arc<Self>,
        jobs: Vec<PrefetchJob>,
    ) -> MailContextResult<()> {
        if jobs.is_empty() {
            tracing::trace!("No prefetch jobs to queue");
            return Ok(());
        }

        let prefetch_service = self.get_service::<PrefetchService>();

        if let Some(sender) = prefetch_service.notify.get() {
            sender.send_async(jobs).await.map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send prefetch signal to prefetcher"))
            })?;

            Ok(())
        } else {
            let (sender, receiver) = flume::bounded(DEFAULT_PREFETCH_BOUND);

            prefetch_service.notify.set(sender).map_err(|e| {
                MailContextError::Other(anyhow!("Failed to set prefetch sender: {e:?}"))
            })?;
            Prefetch::initialize(self.clone(), receiver).await;
            // unwrap safety: `prefetch_service.notify` is set just above, it cannot be `None`.
            prefetch_service
                .notify
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
            .mail_cache_path_for(self.user_id())
            .join("attachment-staging")
    }

    /// See [`UserContext::spawn()`].
    pub fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.user_context.spawn(task)
    }

    pub async fn has_unsent_messages(&self) -> Result<bool, MailContextError> {
        Ok(self
            .action_queue()
            .typed_actions_count::<crate::actions::draft::Send>()
            .await?
            != 0)
    }

    pub async fn has_actions_in_queue(&self) -> Result<bool, MailContextError> {
        Ok(self.action_queue().queued_actions_count().await? != 0)
    }

    pub fn http_client(&self) -> &reqwest::Client {
        self.mail_context().http_client()
    }

    pub fn network_monitor_service(&self) -> &NetworkMonitorService {
        self.mail_context.network_monitor_service()
    }

    pub fn issue_reporter_service(&self) -> &UserIssueReporterService {
        self.user_context.issue_reporter_service()
    }

    pub fn image_loader(&self) -> &ImageLoader {
        self.get_service()
    }
}

impl Drop for MailUserContext {
    fn drop(&mut self) {
        let user_id = self.user_id();
        let session_id = self.session_id();
        tracing::info!(?user_id, ?session_id, "Dropping MailUserContext");
    }
}

impl Spawner for MailUserContext {
    fn spawn_task<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future<Output: Send> + Send + 'static,
    {
        self.spawn(f)
    }
}
