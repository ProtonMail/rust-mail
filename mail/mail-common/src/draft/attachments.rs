use crate::datatypes::AttachmentMetadata;
use crate::models::{
    DraftAttachmentMetadata, DraftAttachmentUploadError, DraftAttachmentUploadState, MetadataId,
};
use proton_mail_ids::LocalAttachmentId;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;

/// Represent an attachment associated with a draft.
pub struct DraftAttachment {
    /// Metadata of the attachment.
    pub metadata: AttachmentMetadata,
    /// Upload status.
    pub state: DraftAttachmentState,
    /// Timestamp of the state update. It will be 0 for attachment that already exist.
    pub state_modified_timestamp: i64,
}

#[derive(Debug)]
pub enum DraftAttachmentState {
    /// Attachment has not been uploaded.
    Uploading,
    /// Attachment has been uploaded to the server
    Uploaded,
    /// Attachment failed to upload or encrypt.
    Error(DraftAttachmentUploadError),
    /// Could not upload due to lack of network,
    Offline,
}

impl DraftAttachmentState {
    pub fn from_draft_attachment_metadata(metadata: &DraftAttachmentMetadata) -> Self {
        match metadata.state() {
            DraftAttachmentUploadState::Uploading => Self::Uploading,
            DraftAttachmentUploadState::Uploaded => Self::Uploaded,
            DraftAttachmentUploadState::Error => {
                let error = metadata
                    .error
                    .clone()
                    .unwrap_or(DraftAttachmentUploadError::Unexpected);
                Self::Error(error)
            }
            DraftAttachmentUploadState::Offline => Self::Offline,
        }
    }
}

impl DraftAttachment {
    /// Merge the list of `attachments` with the attachment data associated with the draft
    /// with `metadata_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn build_list(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<DraftAttachment>, StashError> {
        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(metadata_id, tether).await?;
        let metadata_map: HashMap<LocalAttachmentId, DraftAttachmentMetadata> = HashMap::from_iter(
            DraftAttachmentMetadata::find_by_metadata_id(metadata_id, tether)
                .await?
                .into_iter()
                .map(|m| (m.local_attachment_id, m)),
        );

        Ok(attachments
            .into_iter()
            .map(|attachment| {
                let (state, timestamp) =
                    if let Some(metadata) = metadata_map.get(&attachment.local_id.unwrap()) {
                        (
                            DraftAttachmentState::from_draft_attachment_metadata(metadata),
                            metadata.state_timestamp(),
                        )
                    } else {
                        // If there is no metadata entry, it means there are no changes for this attachment
                        // or it was inherited from a reply/forward.
                        (DraftAttachmentState::Uploaded, 0)
                    };
                DraftAttachment {
                    state,
                    metadata: AttachmentMetadata::from(attachment),
                    state_modified_timestamp: timestamp,
                }
            })
            .collect())
    }
}
