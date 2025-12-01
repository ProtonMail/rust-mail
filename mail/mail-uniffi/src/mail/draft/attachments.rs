use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{
    DraftAttachmentDispositionSwapError, DraftAttachmentDispositionSwapErrorReason,
    DraftAttachmentRetryError, DraftAttachmentUploadError, DraftAttachmentUploadErrorReason,
    ProtonError, VoidDraftAttachmentDispositionSwapResult, VoidProtonResult,
};
use crate::mail::datatypes::AttachmentMetadata;
use crate::mail::state::MailUserContextPtr;
use crate::{AsyncLiveQueryCallback, uniffi_async};
use anyhow::anyhow;
use proton_mail_common::MailContextError;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::datatypes::{Disposition, LocalAttachmentId};
use proton_mail_common::draft::Draft as RealDraft;
use proton_mail_common::draft::attachments::{
    DraftAttachment as RealDraftAttachment,
    DraftAttachmentDispositionSwapError as RealDraftAttachmentDispositionSwapError,
    DraftAttachmentError as RealDraftAttachmentError,
    DraftAttachmentState as RealDraftAttachmentState,
    DraftAttachmentUploadError as RealDraftAttachmentUploadError,
};
use proton_mail_common::draft::observers::DraftAttachmentObserver;
use proton_mail_common::models::Attachment as RealAttachment;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;
use tracing::error;
use uniffi_common::errors::UserApiServiceError;
use uniffi_runtime::async_runtime;

#[derive(uniffi::Enum)]
pub enum DraftAttachmentError {
    Upload(DraftAttachmentUploadError),
    DispositionSwap(DraftAttachmentDispositionSwapError),
}

impl From<RealDraftAttachmentError> for DraftAttachmentError {
    fn from(err: RealDraftAttachmentError) -> Self {
        match err {
            RealDraftAttachmentError::Upload(e) => Self::Upload(e.into()),
            RealDraftAttachmentError::DispositionSwap(e) => Self::DispositionSwap(e.into()),
        }
    }
}
/// State of the attachment
#[derive(uniffi::Enum)]
pub enum DraftAttachmentState {
    /// Can't upload due to lack of network.
    Offline,
    /// Attachment is uploading.
    Uploading,
    /// Attachment has failed uploading
    Uploaded,
    /// An error occurred during upload.
    Error(DraftAttachmentError),
    /// Attachment is awaiting upload
    Pending,
}

impl From<RealDraftAttachmentState> for DraftAttachmentState {
    fn from(value: RealDraftAttachmentState) -> Self {
        match value {
            RealDraftAttachmentState::Uploading => Self::Uploading,
            RealDraftAttachmentState::Uploaded => Self::Uploaded,
            RealDraftAttachmentState::Error(e) => Self::Error(e.into()),
            RealDraftAttachmentState::Offline => Self::Offline,
            RealDraftAttachmentState::Pending => Self::Pending,
        }
    }
}

impl From<RealDraftAttachmentUploadError> for DraftAttachmentUploadError {
    fn from(value: RealDraftAttachmentUploadError) -> Self {
        match value {
            RealDraftAttachmentUploadError::Crypto(_) => {
                Self::Reason(DraftAttachmentUploadErrorReason::Crypto)
            }
            RealDraftAttachmentUploadError::TooManyAttachments => {
                Self::Reason(DraftAttachmentUploadErrorReason::TooManyAttachments)
            }
            RealDraftAttachmentUploadError::MessageAlreadySent => {
                Self::Reason(DraftAttachmentUploadErrorReason::MessageAlreadySent)
            }
            RealDraftAttachmentUploadError::Server(e) => {
                // There is no good conversion here, however it should be very rare as all
                // the important cases are intercepted.
                Self::Other(ProtonError::ServerError(
                    UserApiServiceError::OtherHttpError(0, e),
                ))
            }
            RealDraftAttachmentUploadError::Unexpected => {
                Self::Other(ProtonError::Unexpected(UnexpectedError::Draft))
            }
            RealDraftAttachmentUploadError::AttachmentTooLarge => {
                Self::Reason(DraftAttachmentUploadErrorReason::AttachmentTooLarge)
            }
            RealDraftAttachmentUploadError::TotalAttachmentsTooLarge => {
                Self::Reason(DraftAttachmentUploadErrorReason::TotalAttachmentSizeTooLarge)
            }
        }
    }
}

impl From<RealDraftAttachmentDispositionSwapError> for DraftAttachmentDispositionSwapError {
    fn from(err: RealDraftAttachmentDispositionSwapError) -> Self {
        match err {
            RealDraftAttachmentDispositionSwapError::Server(e) => {
                // There is no good conversion here, however it should be very rare as all
                // the important cases are intercepted.
                Self::Other(ProtonError::ServerError(
                    UserApiServiceError::OtherHttpError(0, e),
                ))
            }
            RealDraftAttachmentDispositionSwapError::AttachmentNotFound => {
                Self::Reason(DraftAttachmentDispositionSwapErrorReason::AttachmentDoesNotExist)
            }
            RealDraftAttachmentDispositionSwapError::AttachmentMessageNotFound => Self::Reason(
                DraftAttachmentDispositionSwapErrorReason::AttachmentMessageDoesNotExist,
            ),
            RealDraftAttachmentDispositionSwapError::AttachmentMessageIsNotADraft => Self::Reason(
                DraftAttachmentDispositionSwapErrorReason::AttachmentMessageIsNotADraft,
            ),
            RealDraftAttachmentDispositionSwapError::Unexpected => {
                Self::Other(ProtonError::Unexpected(UnexpectedError::Draft))
            }
        }
    }
}

/// Represents a attachment associated with a draft.
#[derive(uniffi::Record)]
pub struct DraftAttachment {
    /// The state at which this attachment finds itself.
    pub state: DraftAttachmentState,
    /// Metadata related to the attachment.
    pub attachment: AttachmentMetadata,
    /// Timestamp of the status change
    pub state_modified_timestamp: i64,
}

impl From<RealDraftAttachment> for DraftAttachment {
    fn from(attachment: RealDraftAttachment) -> Self {
        Self {
            state: attachment.state.into(),
            attachment: attachment.metadata.into(),
            state_modified_timestamp: attachment.state_modified_timestamp,
        }
    }
}

/// Access and modify the [`Draft`]'s attachments.
#[derive(uniffi::Object)]
pub struct AttachmentList {
    ctx: MailUserContextPtr,
    staging_path: String,
    draft: RealDraft,
}

impl AttachmentList {
    pub(crate) fn new(ctx: MailUserContextPtr, staging_path: &Path, draft: RealDraft) -> Arc<Self> {
        Arc::new(Self {
            ctx,
            staging_path: staging_path.to_string_lossy().into_owned(),
            draft,
        })
    }
}

#[uniffi_export]
impl AttachmentList {
    /// Add a new attachment to this draft. If `filename_override` is present, that will become
    /// the filename of the attachment. Otherwise, it is extracted from the path.
    pub async fn add(
        self: Arc<Self>,
        path: String,
        filename_override: Option<String>,
    ) -> Result<(), DraftAttachmentUploadError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftAttachmentUploadError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async::<(), RealProtonMailError, _>(async move {
            let path = PathBuf::from(path);

            let address_id = self.draft.address_id().await?;
            let mut tether = ctx.user_stash().connection().await?;

            let result = RealAttachment::create_local(
                &ctx,
                address_id,
                Disposition::Attachment,
                &path,
                filename_override,
                &mut tether,
            )
            .await;

            self.draft
                .delete_attachment_if_in_staging_area(&ctx, &path)
                .await;
            let attachment = result?;
            self.draft.add_attachment(&attachment).await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    /// Add a new inline attachment to this draft. If `filename_override` is present, that will become
    /// the filename of the attachment. Otherwise, it is extracted from the path.
    ///
    /// Returns the assigned content id.
    pub async fn add_inline(
        self: Arc<Self>,
        path: String,
        filename_override: Option<String>,
    ) -> Result<String, DraftAttachmentUploadError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftAttachmentUploadError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };

        uniffi_async::<String, RealProtonMailError, _>(async move {
            let path = PathBuf::from(path);

            let address_id = self.draft.address_id().await?;
            let mut tether = ctx.user_stash().connection().await?;

            let result = RealAttachment::create_local(
                &ctx,
                address_id,
                Disposition::Inline,
                &path,
                filename_override,
                &mut tether,
            )
            .await;

            self.draft
                .delete_attachment_if_in_staging_area(&ctx, &path)
                .await;
            let attachment = result?;
            let content_id = attachment
                .content_id
                .clone()
                .ok_or(MailContextError::Other(anyhow!(
                    "Somehow missing attachment content id"
                )))?;
            self.draft.add_attachment(&attachment).await?;
            Ok(content_id.into_inner())
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    /// Remove an attachment from this draft.
    pub async fn remove(self: Arc<Self>, id: Id) -> Result<(), ProtonError> {
        let id: LocalAttachmentId = id.into();
        uniffi_async::<(), RealProtonMailError, _>(async move {
            self.draft.remove_attachment(id).await?;
            Ok(())
        })
        .await
        .map_err(ProtonError::from)
    }

    /// Remove an attachment from this draft by `content-id`.
    pub async fn remove_with_cid(
        self: Arc<Self>,
        content_id: String,
    ) -> Result<(), DraftAttachmentUploadError> {
        let id = ContentId::from(content_id);
        uniffi_async::<(), RealProtonMailError, _>(async move {
            self.draft.remove_attachment_with_cid(id).await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    /// Retry the upload of a failed attachment.
    pub async fn retry(
        self: Arc<Self>,
        attachment_id: Id,
    ) -> Result<(), DraftAttachmentRetryError> {
        uniffi_async::<(), RealProtonMailError, _>(async move {
            self.draft
                .retry_attachment_action(attachment_id.into())
                .await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentRetryError::from)
    }

    /// Get the directory for attachment uploads.
    pub fn attachment_upload_directory(&self) -> String {
        self.staging_path.clone()
    }

    /// Get the list of attachments.
    pub async fn attachments(
        self: Arc<Self>,
    ) -> Result<Vec<DraftAttachment>, DraftAttachmentUploadError> {
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let attachments = self.draft.attachments().await?;
            Ok(attachments.into_iter().map(DraftAttachment::from).collect())
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    /// Create a new watcher for attachment status updates..
    pub async fn watcher(
        self: Arc<Self>,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<DraftAttachmentWatcher>, DraftAttachmentUploadError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftAttachmentUploadError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let metadata_id = self.draft.metadata_id;
            let stash = ctx.user_stash().clone();
            let mut observer = DraftAttachmentObserver::new(metadata_id, stash)
                .await
                .map_err(RealProtonMailError::from)?;
            let handle = ctx.spawn(async move {
                loop {
                    match observer.next().await {
                        Ok(()) => {
                            callback.on_update().await;
                        }
                        Err(e) => {
                            error!("Draft attachment observer failed: {e:?}");
                            return;
                        }
                    }
                }
            });
            Ok(Arc::new(DraftAttachmentWatcher {
                abort_handle: handle.abort_handle(),
            }))
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    pub async fn watcher_stream(
        self: Arc<Self>,
    ) -> Result<Arc<DraftAttachmentListUpdateStream>, DraftAttachmentUploadError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftAttachmentUploadError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let metadata_id = self.draft.metadata_id;
            let stash = ctx.user_stash().clone();
            let observer = DraftAttachmentObserver::new(metadata_id, stash)
                .await
                .map_err(RealProtonMailError::from)?;
            Ok(Arc::new(DraftAttachmentListUpdateStream {
                observer: tokio::sync::Mutex::new(observer),
                token: CancellationToken::new(),
            }))
        })
        .await
        .map_err(DraftAttachmentUploadError::from)
    }

    #[returns(VoidDraftAttachmentDispositionSwapResult)]
    pub async fn swap_attachment_disposition(
        &self,
        content_id: String,
    ) -> Result<(), DraftAttachmentDispositionSwapError> {
        self.draft
            .swap_attachment_disposition_from_inline(ContentId::from(content_id))
            .await
            .map_err(RealProtonMailError::from)?;
        Ok(())
    }
}

#[derive(uniffi::Object)]
pub struct DraftAttachmentListUpdateStream {
    observer: tokio::sync::Mutex<DraftAttachmentObserver>,
    token: CancellationToken,
}

#[uniffi_export]
impl DraftAttachmentListUpdateStream {
    #[returns(VoidProtonResult)]
    pub async fn next_async(self: Arc<Self>) -> Result<(), ProtonError> {
        async_runtime()
            .spawn(async move {
                let mut observer = self.observer.lock().await;
                let future = observer.next();
                self.token
                    .run_until_cancelled(future)
                    .await
                    .ok_or_else(|| RealProtonMailError::from(MailContextError::TaskCancelled))?
                    .map_err(|e| RealProtonMailError::from(MailContextError::from(e)))
            })
            .await
            .map_err(RealProtonMailError::from)??;
        Ok(())
    }

    #[returns(VoidProtonResult)]
    pub fn next_sync(&self) -> Result<(), ProtonError> {
        async_runtime().block_on(async {
            let mut observer = self.observer.lock().await;
            Ok(observer.next().await.map_err(RealProtonMailError::from)?)
        })
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }
}

/// Observe draft send results.
///
/// Note that this will only notify you of new records that have not been seen before.
#[derive(uniffi::Object)]
pub struct DraftAttachmentWatcher {
    abort_handle: AbortHandle,
}

#[uniffi::export]
impl DraftAttachmentWatcher {
    pub fn disconnect(&self) {
        self.abort_handle.abort();
    }
}

impl Drop for DraftAttachmentWatcher {
    fn drop(&mut self) {
        self.disconnect();
    }
}
