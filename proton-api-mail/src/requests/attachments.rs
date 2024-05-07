use crate::domain::{Attachment, AttachmentId};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http::{ByteResponse, JsonResponse, Method, RequestData, RequestDesc};

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAttachmentRequest {
    pub id: AttachmentId,
}

impl GetAttachmentRequest {
    #[must_use]
    pub fn new(attachment_id: AttachmentId) -> Self {
        Self { id: attachment_id }
    }
}

impl RequestDesc for GetAttachmentRequest {
    type Response = ByteResponse;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("mail/v4/attachments/{}", self.id))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAttachmentMetadataRequest {
    pub id: AttachmentId,
}

impl GetAttachmentMetadataRequest {
    #[must_use]
    pub fn new(attachment_id: AttachmentId) -> Self {
        Self { id: attachment_id }
    }
}

impl RequestDesc for GetAttachmentMetadataRequest {
    type Response = JsonResponse<GetAttachmentMetadataResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(
            Method::Get,
            format!("mail/v4/attachments/{}/metadata", self.id),
        )
    }
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetAttachmentMetadataResponse {
    pub response: Attachment,
}
