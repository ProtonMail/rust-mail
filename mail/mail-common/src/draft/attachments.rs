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
                let status = if let Some(metadata) = metadata_map.get(&attachment.local_id.unwrap())
                {
                    metadata.state()
                } else {
                    // If there is no metadata entry, it means there are no changes for this attachment
                    // or it was inherited from a reply/forward.
                    DraftAttachmentUploadState::Uploaded
                };
                DraftAttachment {
                    state: status,
                    metadata: AttachmentMetadata::from(attachment),
                }
            })
            .collect())
    }
}
