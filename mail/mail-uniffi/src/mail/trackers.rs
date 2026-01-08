use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ProtonError, UserSessionError};
use crate::mail::datatypes::TrackerInfo;
use crate::mail::user_session::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::TrackerDetector;
use proton_mail_common::{MailContextError, ProtonMailError as RealProtonMailError};
use stash::stash::WatcherHandle;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uniffi_runtime::async_runtime;

#[uniffi_export]
pub async fn get_tracker_info_for_message(
    session: &MailUserSession,
    message_id: Id,
) -> Result<Option<TrackerInfo>, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let result = ctx
            .get_service::<TrackerDetector>()
            .get_tracker_info(message_id.into())
            .await?
            .map(Into::into);

        Ok(result)
    })
    .await
    .map_err(UserSessionError::from)
}

#[uniffi_export]
pub async fn watch_tracker_info_stream(
    session: &MailUserSession,
    message_id: Id,
) -> Result<Arc<WatchTrackerInfoStream>, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let (info, handle) = ctx
            .get_service::<TrackerDetector>()
            .watch(message_id.into())
            .await?;
        Ok::<_, RealProtonMailError>(Arc::new(WatchTrackerInfoStream {
            initial_info: info.map(Into::into),
            handle,
            token: ctx.user_context().create_child_cancellation_token(),
        }))
    })
    .await
    .map_err(UserSessionError::from)
}

#[derive(uniffi::Object)]
pub struct WatchTrackerInfoStream {
    initial_info: Option<TrackerInfo>,
    handle: WatcherHandle,
    token: CancellationToken,
}

#[uniffi_export]
impl WatchTrackerInfoStream {
    #[must_use]
    pub fn initial_info(&self) -> Option<TrackerInfo> {
        self.initial_info.clone()
    }

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
