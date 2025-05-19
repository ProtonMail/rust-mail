use crate::core::datatypes::{Id, UnixTimestamp};
use crate::errors::{
    DraftAttachmentDispositionSwapErrorReason, DraftAttachmentUploadErrorReason,
    DraftSaveErrorReason, DraftSendErrorReason, ProtonError, VoidProtonResult,
};
use crate::mail::MailUserSession;
use crate::{async_runtime, uniffi_async};
use proton_core_common::utils::MapVec;
use proton_mail_common::MailContextError;
use proton_mail_common::datatypes::LocalMessageId;
use proton_mail_common::draft::observers::{
    DraftSendResultWatcher as RealDraftSendResultWatcher, DraftSendResultWatcherMode,
};
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::{
    DraftSendResult as RealDraftSendResult, DraftSendResultOrigin as RealDraftSendResultOrigin,
};
use std::sync::Arc;
use tokio::task::AbortHandle;
use tracing::error;

/// Origin of the result.
#[derive(uniffi::Enum)]
pub enum DraftSendResultOrigin {
    /// When saving a draft without the intention to send.
    Save,
    /// When saving a draft with intention to send.
    SaveBeforeSend,
    /// Sending of the saved draft.
    Send,
    /// When uploading an attachment.
    AttachmentUpload,
    /// We failed when scheduling a message send
    ScheduleSend,
    /// We failed when attempting to swap an attachment disposition
    AttachmentDispositionSwap,
}

impl From<RealDraftSendResultOrigin> for DraftSendResultOrigin {
    fn from(value: RealDraftSendResultOrigin) -> Self {
        match value {
            RealDraftSendResultOrigin::Save => Self::Save,
            RealDraftSendResultOrigin::SaveBeforeSend => Self::SaveBeforeSend,
            RealDraftSendResultOrigin::Send => Self::Send,
            RealDraftSendResultOrigin::AttachmentUpload => Self::AttachmentUpload,
            RealDraftSendResultOrigin::ScheduleSend => Self::ScheduleSend,
            RealDraftSendResultOrigin::AttachmentDispositionSwap => Self::AttachmentDispositionSwap,
        }
    }
}

/// Indicates how a draft operation completed.
#[derive(uniffi::Enum)]
pub enum DraftSendStatus {
    /// Everything was completed with success. Contains the number of seconds left
    /// until the message's sending can be cancelled. `0` means it is no longer
    /// possible or the operation can not be done.
    Success {
        seconds_until_cancel: u64,
        delivery_time: UnixTimestamp,
    },
    /// Something failed.
    Failure(DraftSendFailure),
}

#[derive(uniffi::Enum)]
pub enum DraftSendFailure {
    Save(DraftSaveErrorReason),
    Send(DraftSendErrorReason),
    AttachmentUpload(DraftAttachmentUploadErrorReason),
    AttachmentDispositionSwap(DraftAttachmentDispositionSwapErrorReason),
    Other(ProtonError),
}

/// Result of sending a draft
#[derive(uniffi::Record)]
pub struct DraftSendResult {
    //TODO(ET-1843): Undo send token from remote id if applicable.
    /// The id of draft message
    pub message_id: Id,
    /// Timestamp at which the operation recorded
    pub timestamp: UnixTimestamp,
    /// Success or failure status.
    pub error: DraftSendStatus,
    /// Where this report originated from.
    pub origin: DraftSendResultOrigin,
}

impl From<RealDraftSendResult> for DraftSendResult {
    fn from(value: RealDraftSendResult) -> Self {
        let second_left_for_undo = value.time_left_for_undo().as_secs();
        Self {
            message_id: value.local_message_id.into(),
            timestamp: value.timestamp.into(),
            error: value.error.map_or(
                DraftSendStatus::Success {
                    seconds_until_cancel: second_left_for_undo,
                    delivery_time: value.undo_timestamp.into(),
                },
                |e| {
                    let proton_error = RealProtonMailError::from(e);
                    match proton_error {
                        RealProtonMailError::Reason(RealMailErrorReason::DraftSendReason(e)) => {
                            DraftSendStatus::Failure(DraftSendFailure::Send(e.into()))
                        }
                        RealProtonMailError::Reason(RealMailErrorReason::DraftSaveReason(e)) => {
                            DraftSendStatus::Failure(DraftSendFailure::Save(e.into()))
                        }
                        RealProtonMailError::Reason(
                            RealMailErrorReason::DraftAttachmentUploadReason(e),
                        ) => DraftSendStatus::Failure(DraftSendFailure::AttachmentUpload(e.into())),
                        RealProtonMailError::Reason(
                            RealMailErrorReason::DraftAttachmentDispositionSwapError(e),
                        ) => DraftSendStatus::Failure(DraftSendFailure::AttachmentDispositionSwap(
                            e.into(),
                        )),
                        _ => DraftSendStatus::Failure(DraftSendFailure::Other(proton_error.into())),
                    }
                },
            ),
            origin: value.origin.into(),
        }
    }
}

/// Callback interface to be notified of new draft send results.
#[uniffi::export(with_foreign)]
pub trait DraftSendResultCallback: Send + Sync {
    /// Will be invoked with a list of at least 1 new send result.
    fn on_new_send_result(&self, details: Vec<DraftSendResult>);
}

/// Observe draft send results.
///
/// Note that this will only notify you of new records that have not been seen before.
#[derive(uniffi::Object)]
pub struct DraftSendResultWatcher {
    abort_handle: AbortHandle,
}

/// Create new instance of the watcher for the `session` with `callback`.
#[uniffi_export]
pub async fn new_draft_send_watcher(
    session: Arc<MailUserSession>,
    callback: Arc<dyn DraftSendResultCallback>,
) -> Result<Arc<DraftSendResultWatcher>, ProtonError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let mut observer = RealDraftSendResultWatcher::new(
            ctx.user_stash().clone(),
            DraftSendResultWatcherMode::SentOnly,
        )
        .await?;
        let handle = async_runtime()
            .spawn(async move {
                loop {
                    match observer.next().await {
                        Ok(results) => {
                            let callback = callback.clone();
                            async_runtime().spawn_blocking(move || {
                                callback.on_new_send_result(results.map_vec());
                            });
                        }
                        Err(e) => {
                            error!("Draft Send Result observer error: {:?}", e);
                            return;
                        }
                    }
                }
            })
            .abort_handle();
        Ok::<_, RealProtonMailError>(Arc::new(DraftSendResultWatcher {
            abort_handle: handle,
        }))
    })
    .await
    .map_err(ProtonError::from)
    .into()
}

#[uniffi_export]
impl DraftSendResultWatcher {
    /// Disconnect the watcher and stop observing the table.
    pub fn disconnect(&self) {
        self.abort_handle.abort();
    }
}

impl Drop for DraftSendResultWatcher {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Return all unseen send results for drafts.
///
/// # Errors
///
/// Returns error if the query failed.
#[uniffi_export]
pub async fn draft_send_result_unseen(
    session: &MailUserSession,
) -> Result<Vec<DraftSendResult>, ProtonError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let connection = ctx.user_stash().connection().await?;
        RealDraftSendResult::unseen(&connection)
            .await
            .map(MapVec::map_vec)
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ProtonError::from)
}

/// Mark the send results for the `message_ids` as seen.
///
/// # Errors
///
/// Returns error if the query failed.
#[uniffi_export]
#[returns(VoidProtonResult)]
pub async fn draft_send_result_mark_seen(
    session: &MailUserSession,
    message_ids: Vec<Id>,
) -> Result<(), ProtonError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let mut connection = ctx.user_stash().connection().await?;
        connection
            .tx(async |tx| {
                RealDraftSendResult::mark_seen(
                    message_ids.into_iter().map(LocalMessageId::from),
                    &tx,
                )
                .await
            })
            .await?;
        Ok(())
    })
    .await
    .map_err(|e: MailContextError| ProtonError::from(RealProtonMailError::from(e)))
    .into()
}

/// Delete the send results for the `message_ids`.
///
/// # Errors
///
/// Returns error if the query failed.
#[uniffi_export]
#[returns(VoidProtonResult)]
pub async fn draft_send_result_delete(
    session: &MailUserSession,
    message_ids: Vec<Id>,
) -> Result<(), ProtonError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let mut connection = ctx.user_stash().connection().await?;
        connection
            .tx(async |tx| {
                RealDraftSendResult::delete(message_ids.into_iter().map(LocalMessageId::from), &tx)
                    .await
            })
            .await?;
        Ok(())
    })
    .await
    .map_err(|e: MailContextError| ProtonError::from(RealProtonMailError::from(e)))
    .into()
}
