use proton_api_core::http;

use crate::{
    domain::AttachmentId,
    requests::{
        GetAttachmentMetadataRequest, GetAttachmentMetadataResponse, GetAttachmentRequest,
        GetAttachmentResponse,
    },
    MailSession,
};

impl MailSession {
    pub async fn get_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<GetAttachmentResponse, http::RequestError> {
        self.session()
            .execute_request(GetAttachmentRequest::new(attachment_id))
            .await
    }

    pub async fn get_attachment_metadata(
        &self,
        attachment_id: AttachmentId,
    ) -> Result<GetAttachmentMetadataResponse, http::RequestError> {
        self.session()
            .execute_request(GetAttachmentMetadataRequest::new(attachment_id))
            .await
    }
}
