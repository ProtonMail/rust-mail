mod events;
mod images;
mod labels;

use crate::core::datatypes::{
    AccountDetails, ConnectionStatus, GetPaymentsPlansOptions, Id, NewSubscription,
    NewSubscriptionValues, PaymentMethod, PaymentReceipt, PaymentToken, PaymentsPlans,
    PaymentsStatus, Subscriptions, UpsellEligibility, User, UserSettings,
};
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ActionError, ProtonError, UserSessionError, VoidSessionResult};
use crate::mail::state::MailUserContextPtr;
use crate::{
    AsyncLiveQueryCallback, WatchHandle, async_runtime, declare_live_query_tagger, uniffi_async,
    watch_table,
};
use futures::TryFutureExt;
use muon::common::IntoDyn;
use proton_account_api::login::state::want_qr_confirmation::process_target_device_qr_code;
use proton_account_uniffi::login::ProcessTargetDeviceQrError;
use proton_account_uniffi::password::PasswordFlow;
use proton_account_uniffi::password_validator::PasswordValidatorService;
use proton_core_api::services::proton::ProtonAuth;
use proton_core_common::UserContext;
use proton_core_common::actions::user_feature_flags::OverrideFlag;
use proton_core_common::services::PaymentsService;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::Attachment;
use proton_mail_common::{MailContextError, MailUserContext};
use proton_observability::PreLoginMetricRecorder;
use stash::stash::{Stash, WatcherHandle};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;

use super::datatypes::AttachmentMetadata;

#[derive(uniffi::Object)]
pub struct MailUserSession {
    ctx: MailUserContextPtr,
}

impl MailUserSession {
    pub(crate) fn new(ctx: MailUserContextPtr) -> Arc<Self> {
        Arc::new(Self { ctx })
    }

    pub(crate) fn ptr(&self) -> MailUserContextPtr {
        self.ctx.clone()
    }

    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    pub(crate) fn take_ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.consume().ok_or(UnexpectedError::Internal)?)
    }

    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}

#[uniffi_export]
impl MailUserSession {
    #[must_use]
    pub fn user_id(&self) -> Result<String, ProtonError> {
        Ok(self.ctx()?.user_id().to_owned().into_inner())
    }

    #[must_use]
    pub fn session_id(&self) -> Result<String, ProtonError> {
        Ok(self.ctx()?.session_id().to_owned().into_inner())
    }

    /// Log out a session and delete all user data.
    #[returns(VoidSessionResult)]
    pub async fn logout(&self) -> Result<(), UserSessionError> {
        let ctx = self.take_ctx()?;

        uniffi_async(async move {
            ctx.logout_and_delete_data()
                .map_err(RealProtonMailError::from)
                .await
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }
}

declare_live_query_tagger!(WatchAddressesMarker);
declare_live_query_tagger!(WatchUserMarker);
declare_live_query_tagger!(WatchUserSettingsMarker);
declare_live_query_tagger!(WatchLabelsMarker);
declare_live_query_tagger!(WatchUpsellEligibilityMarker);

#[uniffi_export]
impl MailUserSession {
    pub fn watch_addresses(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        let uctx = ctx.user_context();
        async_runtime().block_on(async {
            watch_table!(
                WatchAddressesMarker,
                uctx,
                ctx,
                callback,
                UserContext::watch_addresses
            )
        })
    }

    pub fn watch_user(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        let uctx = ctx.user_context();
        async_runtime().block_on(async {
            watch_table!(
                WatchUserMarker,
                uctx,
                ctx,
                callback,
                UserContext::watch_user
            )
        })
    }

    pub fn watch_user_settings(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        let uctx = ctx.user_context();
        async_runtime().block_on(async {
            watch_table!(
                WatchUserSettingsMarker,
                uctx,
                ctx,
                callback,
                UserContext::watch_user_settings
            )
        })
    }

    pub fn watch_labels(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        let uctx = ctx.user_context();
        async_runtime().block_on(async {
            watch_table!(
                WatchLabelsMarker,
                uctx,
                ctx,
                callback,
                UserContext::watch_labels
            )
        })
    }

    pub fn watch_upsell_eligibility(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        async_runtime().block_on(async {
            watch_table!(
                WatchUpsellEligibilityMarker,
                ctx.as_ref(),
                ctx,
                callback,
                MailUserContext::watch_upsell_eligibility
            )
        })
    }

    pub fn watch_user_stream(&self) -> Result<Arc<WatchUserStream>, ProtonError> {
        let ctx = self.ctx()?;
        async_runtime().block_on(async {
            let handle = ctx
                .user_context()
                .watch_user()
                .await
                .map_err(RealProtonMailError::from)?;
            Ok(Arc::new(WatchUserStream {
                handle,
                token: ctx.user_context().create_child_cancellation_token(),
            }))
        })
    }
}

#[derive(uniffi::Object)]
pub struct WatchUserStream {
    handle: WatcherHandle,
    token: CancellationToken,
}

#[uniffi_export]
impl WatchUserStream {
    pub async fn next_async(self: Arc<Self>) -> Result<(), ProtonError> {
        async_runtime()
            .spawn(async move {
                let future = self.handle.receiver.recv_async();
                self.token
                    .run_until_cancelled(future)
                    .await
                    .ok_or_else(|| RealProtonMailError::from(MailContextError::TaskCancelled))?
                    .map_err(|_| ProtonError::Unexpected(UnexpectedError::Internal))
            })
            .await
            .map_err(RealProtonMailError::from)??;
        Ok(())
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }
}

#[uniffi_export]
impl MailUserSession {
    pub async fn session_uuid(&self) -> Result<String, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx.session().get_sessions_uuid().await?;

            Result::<_, RealProtonMailError>::Ok(res.uuid)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Fork the current session for a child with the given platform and product.
    ///
    /// This call has to be made from a parent session, and forks the current
    /// logged-in user session in order to provide a new session for the same
    /// user.
    ///
    /// If successful, this will return the "Selector" string for the new
    /// session. The child must present an app version that matches the platform
    /// and product.
    ///
    pub async fn fork(
        &self,
        platform: String,
        product: String,
    ) -> Result<String, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            ctx.session()
                .fork(platform, product)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn new_password_change_flow(&self) -> Result<Arc<PasswordFlow>, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            ctx.new_password_change_flow()
                .await
                .map(|flow| PasswordFlow::new(flow))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Processes a QR code from a Target Device to initiate a secure session fork.
    ///
    /// This function parses the provided QR code, retrieves the current device's session passphrase,
    /// optionally encrypts it using the encryption key from the QR code, and sends a fork confirmation
    /// to the Target Device.
    pub async fn process_target_device_qr_code(
        &self,
        qr_code: String,
    ) -> Result<(), ProcessTargetDeviceQrError> {
        let ctx = self
            .ctx()
            .inspect_err(|err| error!("Failed to get Context: {err:?}"))
            .map_err(|_| {
                ProcessTargetDeviceQrError::Other(String::from("Failed to get Context"))
            })?;
        let session = ctx.session().to_owned();
        let (client, _) = session.into_parts();
        let core_context = Arc::clone(ctx.core_context());
        let observability = PreLoginMetricRecorder::default();
        uniffi_async::<_, ProcessTargetDeviceQrError, _>(async move {
            process_target_device_qr_code(&qr_code, client, core_context, observability)
                .await
                .map_err(ProcessTargetDeviceQrError::from)
        })
        .await
        .into()
    }

    #[must_use]
    pub fn password_validator(&self) -> Option<Arc<PasswordValidatorService>> {
        let ctx = self
            .ctx()
            .inspect_err(|err| error!("Failed to get Context: {err:?}"))
            .ok()?;
        Some(Arc::new(PasswordValidatorService::setup(
            ctx.session().to_owned().into_dyn(),
        )))
    }

    pub async fn user(&self) -> Result<User, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let user = ctx.user().await?;
            Result::<_, RealProtonMailError>::Ok(user.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn account_details(&self) -> Result<AccountDetails, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let account_details = ctx.account_details().await?;
            Result::<_, RealProtonMailError>::Ok(account_details.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn user_settings(&self) -> Result<UserSettings, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let settings = ctx.user_settings().ok_into().await?;

            Result::<_, RealProtonMailError>::Ok(settings)
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn get_attachment(
        &self,
        local_attachment_id: Id,
    ) -> Result<DecryptedAttachment, ActionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let mut tether = ctx.user_stash().connection().await?;
            Attachment::get_attachment(&ctx, local_attachment_id.into(), &mut tether)
                .await
                .map(DecryptedAttachment::try_from)?
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub async fn connection_status(&self) -> Result<ConnectionStatus, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let status = ctx.connection_status().into();
            // unfortunatelly there is join error here which need to be handled
            Result::<ConnectionStatus, RealProtonMailError>::Ok(status)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Execute callback when connection status is online
    ///
    /// The method will execute callback immediately when current status is online
    /// otherwise it will wait till the status is online again and then execute callback
    ///
    pub fn execute_when_online(&self, callback: Arc<dyn ExecuteWhenOnlineCallback>) {
        let Ok(ctx) = self.ctx() else {
            tracing::error!("Cannot obtain context, callback will not be executed");
            return;
        };

        let ctx_cloned = ctx.clone();
        ctx.spawn(async move {
            ctx_cloned
                .network_monitor_service()
                .network_status_observer()
                .wait_until_online()
                .await;
            _ = async_runtime()
                .spawn_blocking(move || {
                    callback.on_online();
                })
                .await;
        });
    }

    /// Execute callback when connection status is online
    ///
    /// The method will execute callback immediately when current status is online
    /// otherwise it will wait till the status is online again and then execute callback
    ///
    pub fn execute_when_online_async(&self, callback: Arc<dyn ExecuteWhenOnlineCallbackAsync>) {
        let Ok(ctx) = self.ctx() else {
            tracing::error!("Cannot obtain context, callback will not be executed");
            return;
        };

        let ctx_cloned = ctx.clone();
        ctx.spawn(async move {
            ctx_cloned
                .network_monitor_service()
                .network_status_observer()
                .wait_until_online()
                .await;
            callback.on_online().await;
        });
    }

    pub async fn get_payments_status(
        &self,
        vendor: String,
    ) -> Result<PaymentsStatus, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .get_payments_status(vendor)
                .await
                .map_err(MailContextError::from)?;

            Result::<_, RealProtonMailError>::Ok(PaymentsStatus {
                location: res.location.into(),
                payment_methods: res.payment_methods.into(),
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> Result<PaymentsPlans, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .get_payments_plans(options.into())
                .await
                .map_err(MailContextError::from)?;

            Result::<_, RealProtonMailError>::Ok(PaymentsPlans {
                plans: res.plans.into_iter().map(Into::into).collect(),
                default_cycle: res.default_cycle.into(),
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn get_payments_resources_icons(
        &self,
        name: String,
    ) -> Result<Vec<u8>, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .get_payments_resources_icons(name)
                .await
                .map_err(MailContextError::from)?;

            Result::<_, RealProtonMailError>::Ok(res.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> Result<PaymentToken, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .post_payments_tokens(amount, currency, payment.into())
                .await
                .map_err(MailContextError::from)?;

            Result::<_, RealProtonMailError>::Ok(PaymentToken {
                token: res.token,
                status: res.status,
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn get_payments_subscription(&self) -> Result<Subscriptions, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .get_payments_subscription()
                .await
                .map_err(MailContextError::from)?;
            let current = res.subscriptions.into_iter().map(Into::into);
            let upcoming = res.upcoming_subscriptions.into_iter().map(Into::into);

            Result::<_, RealProtonMailError>::Ok(Subscriptions {
                current: current.collect(),
                upcoming: upcoming.collect(),
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn post_payments_subscription(
        &self,
        subscription: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> Result<(), UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            ctx.user_context()
                .get_service::<PaymentsService>()
                .post_payments_subscription(subscription.into(), new_values.into())
                .await
                .map_err(MailContextError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn get_payment_method(&self, id: String) -> Result<PaymentMethod, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let res = ctx
                .user_context()
                .get_service::<PaymentsService>()
                .get_payment_method(id)
                .await
                .map_err(MailContextError::from)?;

            Ok::<_, RealProtonMailError>(res.payment_method.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn upsell_eligibility(&self) -> Result<UpsellEligibility, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let upsell_eligibility = ctx.upsell_eligibility().await?;
            Result::<_, RealProtonMailError>::Ok(upsell_eligibility.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    pub async fn has_valid_sender_address(&self) -> Result<bool, ProtonError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let address = ctx
                .user_context()
                .address_service()
                .find_valid_sender_address()
                .await
                .map_err(MailContextError::from)?;
            Result::<_, RealProtonMailError>::Ok(address.is_some())
        })
        .await
        .map_err(ProtonError::from)
    }

    /// Is the Unleash OR legacy feature enabled. Returns not only global feature flags,
    /// but also user-specific ones.
    ///
    /// If you don't have an active user session, use [`MailSession::is_feature_enabled`] instead.
    ///
    /// Currently:
    /// * Returns None if feature is not found
    /// * Returns Some(true) if feature is enabled (or present in case of Unleash)
    /// * Returns Some(false) if feature is disabled (only legacy, Unleash returns None in that case)
    ///
    /// If there are two features with the same id, coming from unleash and legacy, unleash takes the precedence.
    pub async fn is_feature_enabled(
        &self,
        feature_id: String,
    ) -> Result<Option<bool>, ProtonError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let flag = ctx
                .user_context()
                .feature_flags()
                .get(&feature_id)
                .await
                .map_err(MailContextError::from)?;

            Ok::<_, RealProtonMailError>(flag)
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    pub fn watch_feature_flags_stream(
        &self,
    ) -> Result<Arc<WatchUserFeatureFlagsStream>, ProtonError> {
        let ctx = self.ctx()?;

        async_runtime().block_on(async {
            let handle = ctx
                .user_context()
                .feature_flags()
                .watch()
                .await
                .map_err(MailContextError::from)
                .map_err(RealProtonMailError::from)?;
            Ok(Arc::new(WatchUserFeatureFlagsStream {
                handle,
                token: ctx.user_context().create_child_cancellation_token(),
            }))
        })
    }

    /// Fails if:
    /// * Feature is missing
    /// * Feature is not writable
    ///     * All Unleash flags are not writable.
    pub async fn override_user_feature_flag(
        &self,
        flag_name: String,
        new_value: bool,
    ) -> Result<(), ActionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let action = OverrideFlag::new(flag_name, new_value);
            ctx.user_context()
                .queue()
                .queue_action(action)
                .await
                .map_err(|e| {
                    RealProtonMailError::Unexpected(
                        anyhow::anyhow!("Feature flag override failed: {e}").into(),
                    )
                })?;
            Ok::<_, RealProtonMailError>(())
        })
        .await
        .map_err(ActionError::from)
    }
}

impl TryFrom<proton_mail_common::DecryptedAttachment> for DecryptedAttachment {
    type Error = anyhow::Error;

    fn try_from(value: proton_mail_common::DecryptedAttachment) -> Result<Self, Self::Error> {
        let data_path = value
            .data_path
            .into_os_string()
            .into_string()
            .map_err(|str| anyhow::anyhow!("Path contained invalid utf8: {str:?}"))?;

        Ok(DecryptedAttachment {
            attachment_metadata: value.attachment_metadata.into(),
            data_path,
        })
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DecryptedAttachment {
    pub attachment_metadata: AttachmentMetadata,
    pub data_path: String,
}

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait ExecuteWhenOnlineCallbackAsync: Send + Sync {
    async fn on_online(&self);
}

#[uniffi::export(with_foreign)]
pub trait ExecuteWhenOnlineCallback: Send + Sync {
    fn on_online(&self);
}

#[derive(uniffi::Object)]
pub struct WatchUserFeatureFlagsStream {
    pub handle: WatcherHandle,
    token: CancellationToken,
}

#[uniffi_export]
impl WatchUserFeatureFlagsStream {
    pub async fn next_async(self: Arc<Self>) -> Result<(), ProtonError> {
        async_runtime()
            .spawn(async move {
                let future = self.handle.receiver.recv_async();
                self.token
                    .run_until_cancelled(future)
                    .await
                    .ok_or_else(|| RealProtonMailError::from(MailContextError::TaskCancelled))?
                    .map_err(|_| ProtonError::Unexpected(UnexpectedError::Internal))
            })
            .await
            .map_err(RealProtonMailError::from)??;
        Ok(())
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }
}
