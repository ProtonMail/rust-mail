mod events;
mod images;
mod labels;

use crate::core::datatypes::{
    AccountDetails, ConnectionStatus, GetPaymentsPlansOptions, Id, NewSubscription,
    NewSubscriptionValues, PaymentReceipt, PaymentToken, PaymentsPlans, Subscriptions, User,
    UserSettings,
};
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ActionError, ProtonError, UserSessionError, VoidSessionResult};
use crate::mail::state::MailUserContextPtr;
use crate::{
    AsyncLiveQueryCallback, WatchHandle, async_runtime, uniffi_async, watch_channel_async,
};
use futures::TryFutureExt;
use muon::common::IntoDyn;
use proton_account_api::login::state::want_qr_confirmation::process_target_device_qr_code;
use proton_account_uniffi::login::ProcessTargetDeviceQrError;
use proton_account_uniffi::password::PasswordFlow;
use proton_account_uniffi::password_validator::PasswordValidatorService;
use proton_core_api::services::observability::ObservabilityRecorder;
use proton_core_api::services::proton::{ProtonAuth, ProtonPayments};
use proton_core_common::UserContext;
use proton_mail_common::MailUserContext;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::Attachment;
use stash::stash::{Stash, StashError, WatcherHandle};
use std::sync::Arc;
use tracing::error;

use super::datatypes::AttachmentMetadata;

/// [`MailUserSession`] represents an active user session.
///
/// This type contains all the relevant information for an active user session.
/// You obtain one by completing the [`crate::mail::LoginFlow`] or restoring an existing session
/// with [`crate::mail::MailSession::user_context_from_session`].
///
/// # Initialization
///
/// Note, obtaining MailUserSession for the first time calls initialization, which:
///
/// * Might take a while (calling API over spotty network)
/// * Might fail (calling API while no network)
///
/// After succesful initialization app will remember the initialization stage and prevent
/// redundant calls.
///
/// # Lifetime
/// This object needs to be kept alive for the duration of an active user session.
#[derive(uniffi::Object)]
pub struct MailUserSession {
    ctx: MailUserContextPtr,
}

impl MailUserSession {
    pub(crate) fn new(ctx: MailUserContextPtr) -> Arc<Self> {
        Arc::new(Self { ctx })
    }

    /// Get a clone of the inner weak reference to the user context.
    pub(crate) fn ptr(&self) -> MailUserContextPtr {
        self.ctx.clone()
    }

    /// Get a strong reference to the inner user context.
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    /// Take ownership of the inner user context.
    pub(crate) fn take_ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.consume().ok_or(UnexpectedError::Internal)?)
    }

    /// Get the connection to the user database
    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }

    pub fn watch_table(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
        watch_fn: fn(&UserContext) -> Result<WatcherHandle, StashError>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        let ctx = self.ctx()?;
        let watcher_handle = watch_fn(ctx.user_context())
            .inspect_err(|err| error!("Error while getting user_context: {err:?}"))
            .map_err(|_| ProtonError::Unexpected(UnexpectedError::Database))?;
        let watch_handle = watch_channel_async(&*ctx, watcher_handle, callback);
        Ok(watch_handle)
    }
}

#[uniffi_export]
impl MailUserSession {
    /// Get the User ID of the current user.
    #[must_use]
    pub fn user_id(&self) -> Result<String, ProtonError> {
        Ok(self.ctx()?.user_id().to_owned().into_inner())
    }

    /// Get the Session ID of the current user's session.
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

#[uniffi_export]
impl MailUserSession {
    pub fn watch_addresses(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        self.watch_table(callback, UserContext::watch_addresses)
    }

    pub fn watch_user(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        self.watch_table(callback, UserContext::watch_user)
    }

    pub fn watch_user_settings(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        self.watch_table(callback, UserContext::watch_user_settings)
    }

    pub fn watch_labels(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ProtonError> {
        self.watch_table(callback, UserContext::watch_labels)
    }
}

#[uniffi_export]
impl MailUserSession {
    /// Get the session UUID of the active user session.
    ///
    /// # Errors
    ///
    /// Any of the [`UserSessionError`] possibilities could be returned if
    /// there is a problem with the HTTP request.
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
    /// # Errors
    ///
    /// Any of the [`MailSessionError::Http`] possibilities could be returned if
    /// there is a problem with the HTTP request.
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

    /// Start new password change flow for an authenticated user session.
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
        let observability = ObservabilityRecorder::default();
        uniffi_async::<_, ProcessTargetDeviceQrError, _>(async move {
            process_target_device_qr_code(&qr_code, client, core_context, observability)
                .await
                .map_err(ProcessTargetDeviceQrError::from)
        })
        .await
        .into()
    }

    /// Returns a password validator service.
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

    /// Provides a way to get the datatypes::User FFI instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> Result<User, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let user = ctx.user().await?;
            Result::<_, RealProtonMailError>::Ok(user.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Retrieves the account details for the current user session.
    ///
    /// Returns the user's account details (name, email and avatar information) or an error if the operation fails.
    ///
    /// # Errors
    /// - Returns `UserSessionError` if the account details cannot be retrieved.
    pub async fn account_details(&self) -> Result<AccountDetails, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let account_details = ctx.account_details().await?;
            Result::<_, RealProtonMailError>::Ok(account_details.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Retrieves the user's settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn user_settings(&self) -> Result<UserSettings, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let settings = ctx.user_settings().ok_into().await?;

            Result::<_, RealProtonMailError>::Ok(settings)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Loads the metadata and file path for the given local [`attachment_id`]
    /// into a [`DecryptedAttachment`].
    ///
    /// If the attachment is not present on the device it is retrieved from
    /// the server, decrypted and stored in the cache.
    ///
    /// Additionally, attempts to verify any attached signatures with the
    /// sender's keys. The result can be accessed via the [`VerificationResult`]
    /// result return type.
    ///
    /// # Warning
    ///
    /// Signature verification is currently always failing since no sender keys
    /// are fetched yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the encrypted attachment fetching or decryption fails.
    /// Signature verification failures are not returned as errors.
    pub async fn get_attachment(
        &self,
        local_attachment_id: Id,
    ) -> Result<DecryptedAttachment, ActionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            Attachment::get_attachment(&ctx, local_attachment_id.into())
                .await
                .map(DecryptedAttachment::try_from)?
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
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

    /// Get the payment plans available for the current user.
    pub async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> Result<PaymentsPlans, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx.session().get_payments_plans(options.into()).await?;

            Result::<_, RealProtonMailError>::Ok(PaymentsPlans {
                plans: res.plans.into_iter().map(Into::into).collect(),
                default_cycle: res.default_cycle.into(),
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get the icon resource with the given name.
    pub async fn get_payments_resources_icons(
        &self,
        name: String,
    ) -> Result<Vec<u8>, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx.session().get_payments_resources_icons(name).await?;

            Result::<_, RealProtonMailError>::Ok(res.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Post a payment token to the server.
    pub async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> Result<PaymentToken, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx
                .session()
                .post_payments_tokens(amount, currency, payment.into())
                .await?;

            Result::<_, RealProtonMailError>::Ok(PaymentToken {
                token: res.token,
                status: res.status,
            })
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Get the current subscription of the user.
    pub async fn get_payments_subscription(&self) -> Result<Subscriptions, UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            let res = ctx.session().get_payments_subscription().await?;
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

    /// Post a payment subscription to the server.
    pub async fn post_payments_subscription(
        &self,
        subscription: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> Result<(), UserSessionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            ctx.session()
                .post_payments_subscription(subscription.into(), new_values.into())
                .await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
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

/// Returned by [`Mailbox::get_attachment`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
    /// The attachment content.
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
