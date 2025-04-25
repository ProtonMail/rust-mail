use crate::core::datatypes::Id;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{DraftAttachmentError, DraftAttachmentErrorReason, ProtonError};
use crate::mail::datatypes::AttachmentMetadata;
use crate::mail::draft::Draft;
use crate::{AsyncLiveQueryCallback, uniffi_async};
use anyhow::anyhow;
use proton_mail_common::MailContextError;
use proton_mail_common::datatypes::{Disposition, LocalAttachmentId};
use proton_mail_common::draft::attachments::{
    DraftAttachment as RealDraftAttachment, DraftAttachmentState as RealDraftAttachmentState,
};
use proton_mail_common::draft::observers::DraftAttachmentObserver;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::{Attachment as RealAttachment, DraftAttachmentUploadError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use tokio::task::AbortHandle;
use tracing::error;

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

impl From<DraftAttachmentUploadError> for DraftAttachmentError {
    fn from(value: DraftAttachmentUploadError) -> Self {
        match value {
            DraftAttachmentUploadError::Crypto(_) => {
                Self::Reason(DraftAttachmentErrorReason::Crypto)
            }
            DraftAttachmentUploadError::TooManyAttachments => {
                Self::Reason(DraftAttachmentErrorReason::TooManyAttachments)
            }
            DraftAttachmentUploadError::MessageAlreadySent => {
                Self::Reason(DraftAttachmentErrorReason::MessageAlreadySent)
            }
            DraftAttachmentUploadError::Server(e) => {
                // There is no good conversion here, however it should be very rare as all
                // the important cases are intercepted.
                Self::Other(ProtonError::ServerError(
                    UserApiServiceError::OtherHttpError(0, e),
                ))
            }
            DraftAttachmentUploadError::Unexpected => {
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
    staging_path: String,
    draft: Weak<Draft>,
}

impl AttachmentList {
    pub(crate) fn new(staging_path: &Path, draft: Weak<Draft>) -> Arc<Self> {
        Arc::new(Self {
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
        &self,
        path: String,
        filename_override: Option<String>,
    ) -> Result<(), DraftAttachmentError> {
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };

        let Some(ctx) = draft.ctx.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };

        uniffi_async::<(), RealProtonMailError, _>(async move {
            let path = PathBuf::from(path);

            let address_id = {
                let instance = draft.instance.read().await;
                instance.address_id.clone()
            };
            let mut tether = ctx.user_stash().connection();

            let result = RealAttachment::create_local(
                &ctx,
                address_id,
                Disposition::Attachment,
                &path,
                filename_override,
                &mut tether,
            )
            .await;

            let instance = draft.instance.read().await;
            instance
                .delete_attachment_if_in_staging_area(&ctx, &path)
                .await;
            let attachment = result?;
            instance.add_attachment(&ctx, attachment).await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentError::from)
    }

    /// Add a new inline attachment to this draft. If `filename_override` is present, that will become
    /// the filename of the attachment. Otherwise, it is extracted from the path.
    ///
    /// Returns the assigned content id.
    pub async fn add_inline(
        &self,
        path: String,
        filename_override: Option<String>,
    ) -> Result<String, DraftAttachmentError> {
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };

        uniffi_async::<String, RealProtonMailError, _>(async move {
            let path = PathBuf::from(path);

            let address_id = {
                let instance = draft.instance.read().await;
                instance.address_id.clone()
            };
            let mut tether = draft.ctx.user_stash().connection();

            let result = RealAttachment::create_local(
                &draft.ctx,
                address_id,
                Disposition::Inline,
                &path,
                filename_override,
                &mut tether,
            )
            .await;

            let instance = draft.instance.read().await;
            instance
                .delete_attachment_if_in_staging_area(&draft.ctx, &path)
                .await;
            let attachment = result?;
            let content_id = attachment
                .content_id
                .clone()
                .ok_or(MailContextError::Other(anyhow!(
                    "Somehow missing attachment content id"
                )))?;
            instance.add_attachment(&draft.ctx, attachment).await?;
            Ok(content_id)
        })
        .await
        .map_err(DraftAttachmentError::from)
    }

    /// Remove an attachment from this draft.
    pub async fn remove(&self, id: Id) -> Result<(), DraftAttachmentError> {
        let id: LocalAttachmentId = id.into();
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };

        let Some(ctx) = draft.ctx.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };

        uniffi_async::<(), RealProtonMailError, _>(async move {
            let instance = draft.instance.read().await;
            instance.remove_attachment(&ctx, id).await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentError::from)
    }

    /// Retry the upload of a failed attachment.
    ///
    /// # Errors
    ///
    /// Returns error if the attachment is not in the error state or the action could not
    /// be queued.
    pub async fn retry(&self, attachment_id: Id) -> Result<(), DraftAttachmentError> {
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };

        let Some(ctx) = draft.ctx.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };

        uniffi_async::<(), RealProtonMailError, _>(async move {
            let instance = draft.instance.read().await;
            instance
                .retry_attachment_upload(&ctx, attachment_id.into())
                .await?;
            Ok(())
        })
        .await
        .map_err(DraftAttachmentError::from)
    }

    /// Get the directory for attachment uploads.
    pub fn attachment_upload_directory(&self) -> String {
        self.staging_path.clone()
    }

    /// Get the list of attachments.
    pub async fn attachments(
        self: Arc<Self>,
    ) -> Result<Vec<DraftAttachment>, DraftAttachmentError> {
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };

        let Some(ctx) = draft.ctx.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };

        uniffi_async::<_, RealProtonMailError, _>(async move {
            let tether = ctx.user_stash().connection();
            let instance = draft.instance.read().await;
            let attachments = instance.attachments(&tether).await?;
            Ok(attachments.into_iter().map(DraftAttachment::from).collect())
        })
        .await
        .map_err(DraftAttachmentError::from)
    }

    /// Create a new watcher for attachment status updates..
    pub async fn watcher(
        &self,
        callback: Arc<dyn AsyncLiveQueryCallback>,
    ) -> Result<Arc<DraftAttachmentWatcher>, DraftAttachmentError> {
        let Some(draft) = self.draft.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Draft,
            )));
        };
        let Some(ctx) = draft.ctx.upgrade() else {
            return Err(DraftAttachmentError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let instance = draft.instance.read().await;
            let metadata_id = instance.metadata_id.clone();
            drop(instance);
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
        .map_err(DraftAttachmentError::from)
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
