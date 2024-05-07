use bytes::Bytes;
use proton_api_core::http;

use crate::{
    domain::AttachmentId,
    requests::{GetAttachmentMetadataRequest, GetAttachmentMetadataResponse, GetAttachmentRequest},
    MailSession,
};

impl MailSession {
    /// Calls the API to load encrypted attachment content for the given `attachment_id`.
    pub async fn attachment_content(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<Bytes, http::RequestError> {
        self.session()
            .execute_request(GetAttachmentRequest::new(attachment_id))
            .await
    }

    /// Calls the API to load the full attachment metadata for decrypting its content.
    pub async fn attachment_metadata_complete(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<GetAttachmentMetadataResponse, http::RequestError> {
        self.session()
            .execute_request(GetAttachmentMetadataRequest::new(attachment_id))
            .await
    }
}
