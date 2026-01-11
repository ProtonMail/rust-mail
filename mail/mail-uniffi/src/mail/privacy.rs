use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ProtonError, UserSessionError};
use crate::mail::datatypes::PrivacyInfo;
use crate::mail::user_session::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::{MailContextError, ProtonMailError as RealProtonMailError};
use proton_mail_common::{MailUserContext, TrackerService};
use stash::stash::WatcherHandle;
use std::sync::{Arc, Weak};
use tokio_util::sync::CancellationToken;
use uniffi_runtime::async_runtime;

#[uniffi_export]
pub async fn get_privacy_info_for_message(
    session: &MailUserSession,
    message_id: Id,
) -> Result<PrivacyInfo, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let tracker_service = ctx.get_service::<TrackerService>();
        let privacy = tracker_service.get_info(message_id.into()).await?;

        Ok(privacy.into())
    })
    .await
    .map_err(UserSessionError::from)
}

#[uniffi_export]
pub async fn watch_privacy_info_stream(
    session: &MailUserSession,
    message_id: Id,
) -> Result<Arc<WatchPrivacyInfoStream>, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let privacy_watch_data = ctx
            .get_service::<TrackerService>()
            .watch(message_id.into())
            .await?;

        Ok::<_, RealProtonMailError>(Arc::new(WatchPrivacyInfoStream {
            message_id: message_id.into(),
            initial_info: privacy_watch_data.initial.into(),
            handle: privacy_watch_data.handle,
            token: ctx.user_context().create_child_cancellation_token(),
            ctx: ctx.as_weak(),
        }))
    })
    .await
    .map_err(UserSessionError::from)
}

#[derive(uniffi::Object)]
pub struct WatchPrivacyInfoStream {
    message_id: proton_mail_common::datatypes::LocalMessageId,
    initial_info: PrivacyInfo,
    handle: WatcherHandle,
    token: CancellationToken,
    ctx: Weak<MailUserContext>,
}

#[uniffi_export]
impl WatchPrivacyInfoStream {
    #[must_use]
    pub fn initial_info(&self) -> PrivacyInfo {
        self.initial_info.clone()
    }

    pub async fn next_async(self: Arc<Self>) -> Result<PrivacyInfo, ProtonError> {
        async_runtime()
            .spawn(async move {
                let future = self.handle.receiver.recv_async();
                self.token
                    .run_until_cancelled(future)
                    .await
                    .ok_or_else(|| RealProtonMailError::from(MailContextError::TaskCancelled))?
                    .map_err(|_| ProtonError::Unexpected(UnexpectedError::Internal))?;

                // After receiving notification, fetch the updated privacy info
                let ctx = self
                    .ctx
                    .upgrade()
                    .ok_or_else(|| RealProtonMailError::from(MailContextError::MissingContext))?;

                let info = ctx
                    .get_service::<TrackerService>()
                    .get_info(self.message_id)
                    .await
                    .map_err(RealProtonMailError::from)
                    .map(Into::into)?;

                Ok(info)
            })
            .await
            .map_err(RealProtonMailError::from)?
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }
}
