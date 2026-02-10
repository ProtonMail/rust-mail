mod content_id;
mod mime_type;

pub use self::content_id::*;
pub use self::mime_type::*;
use proton_mail_api::services::proton::prelude::NewAttachmentDisposition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CombinedAttachmentDisposition {
    Attachment,
    Inline(ContentId),
}

impl From<CombinedAttachmentDisposition> for NewAttachmentDisposition {
    fn from(value: CombinedAttachmentDisposition) -> Self {
        match value {
            CombinedAttachmentDisposition::Attachment => Self::Attachment,
            CombinedAttachmentDisposition::Inline(content_id) => {
                Self::Inline(content_id.into_inner())
            }
        }
    }
}
