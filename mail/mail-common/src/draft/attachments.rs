use crate::datatypes::AttachmentMetadata;
use crate::models::{Attachment, DraftAttachmentMetadata, DraftAttachmentUploadState, MetadataId};
use proton_mail_ids::LocalAttachmentId;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;

/// Represent an attachment associated with a draft.
pub struct DraftAttachment {
    /// Metadata of the attachment.
    pub metadata: AttachmentMetadata,
    /// Upload status.
    pub state: DraftAttachmentUploadState,
    /// Timestamp of the state update. It will be 0 for attachment that already exist.
    pub state_modified_timestamp: i64,
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
        attachments: impl IntoIterator<Item = Attachment>,
        tether: &Tether,
    ) -> Result<Vec<DraftAttachment>, StashError> {
        let metadata_map: HashMap<LocalAttachmentId, DraftAttachmentMetadata> = HashMap::from_iter(
            DraftAttachmentMetadata::find_by_metadata_id(metadata_id, tether)
                .await?
                .into_iter()
                .map(|m| (m.local_attachment_id, m)),
        );

        Ok(attachments
            .into_iter()
            .map(|attachment| {
                let (status, timestamp) =
                    if let Some(metadata) = metadata_map.get(&attachment.local_id.unwrap()) {
                        (metadata.state(), metadata.state_timestamp())
                    } else {
                        // If there is no metadata entry, it means there are no changes for this attachment
                        // or it was inherited from a reply/forward.
                        (DraftAttachmentUploadState::Uploaded, 0)
                    };
                DraftAttachment {
                    state: status,
                    metadata: AttachmentMetadata::from(attachment),
                    state_modified_timestamp: timestamp,
                }
            })
            .collect())
    }
}
