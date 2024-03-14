use crate::new_u64_type;
use proton_api_mail::domain::{AttachmentId, AttachmentMetadata, Disposition};
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
new_u64_type!(LocalAttachmentId);

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[serde(crate = "self::serde")]
pub struct LocalAttachmentMetadata {
    pub id: LocalAttachmentId,
    pub rid: Option<AttachmentId>,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub disposition: Disposition,
}

impl LocalAttachmentMetadata {
    pub fn from_attachment_metadata(id: LocalAttachmentId, metadata: AttachmentMetadata) -> Self {
        Self {
            id,
            rid: Some(metadata.id),
            name: metadata.name,
            size: metadata.size,
            mime_type: metadata.mime_type,
            disposition: metadata.disposition,
        }
    }
}
