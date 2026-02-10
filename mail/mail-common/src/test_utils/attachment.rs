use crate::datatypes::attachment;
use crate::test_utils::test_context::MailTestContext;
use proton_core_api::services::proton::AddressId;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
use proton_crypto_inbox::attachment::KeyPackets;
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{
    NewAttachmentDisposition, NewAttachmentParams, PutAttachmentDispositionRequest,
};
use proton_mail_api::services::proton::response_data::{
    Attachment as ApiAttachment, AttachmentMetadata as ApiAttachmentMetadata,
    Disposition as ApiDisposition,
};
use proton_mail_api::services::proton::responses::{
    GetAttachmentMetadataResponse, PostAttachmentResponse,
};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate, Times};

const TEST_ATTACHMENT_ID: &str =
    "5OkOlBi3Swa4cHRyChyUazwt8GYDBLIAX-ZYnGg8-nAHNKjj5EgR5uH-GePQFaWQPgS60aoJ1Dl2s6UI4BmwNw==";

/// The metadata for the default attachment.
#[must_use]
pub fn testdata_attachment_metadata() -> ApiAttachmentMetadata {
    ApiAttachmentMetadata {
        id: AttachmentId::from(TEST_ATTACHMENT_ID),
        size: 61,
        name: "attachment.txt".to_owned(),
        mime_type: attachment::MimeType::text_plain().to_string(),
        disposition: ApiDisposition::Attachment,
    }
}

/// The complete metadata for the default attachment.
///
/// The attachment is encrypted with the default test account address key.
#[must_use]
pub fn testdata_attachment_metadata_complete(
    message_id: MessageId,
    conversation_id: ConversationId,
) -> ApiAttachment {
    let metadata = testdata_attachment_metadata();
    ApiAttachment {
        id: metadata.id.clone(),
        name: metadata.name.clone(),
        size: metadata.size,
        mime_type: metadata.mime_type,
        disposition: metadata.disposition,
        key_packets: KeyPackets::from(
            "wV4DGS71hsmM2EQSAQdAFwebQBU6CrI3xDOoDnKxPTNV9OiWHh3b+40HFTJckzows6dP/z/dRsZZPKn/Hg4kH7mJTseFFN6yGJlnx22nNzN/+KGYR5Gb+uEaFbpAZFuC",
        ),
        signature: None,
        enc_signature: None,
        sender: None,
        address_id: AddressId::from(TEST_ADDRESS_ID),
        message_id,
        conversation_id,
        content_id: None,
        is_auto_forwardee: false,
    }
}

/// The encrypted data of the default attachment.
#[must_use]
pub fn testdata_attachment_data() -> Vec<u8> {
    vec![
        210, 59, 1, 75, 249, 106, 13, 153, 197, 164, 144, 235, 96, 92, 106, 220, 206, 208, 189, 17,
        127, 6, 220, 69, 65, 126, 205, 138, 245, 180, 110, 215, 254, 99, 121, 249, 127, 69, 117,
        44, 194, 232, 202, 197, 150, 65, 245, 172, 8, 130, 69, 101, 144, 170, 20, 17, 58, 171, 52,
        247, 130,
    ]
}

/// The expected plaintext content of the default test attachment.
#[must_use]
pub fn testdata_expected_attachment_decrypted() -> Vec<u8> {
    b"attachment".to_vec()
}

impl MailTestContext {
    /// Generate new mock for retrieving complete attachment metadata.
    ///
    /// This function will mock the response for the give attachment metadata.
    ///
    #[function_name::named]
    pub async fn mock_get_attachment_metadata(
        &self,
        attachment: ApiAttachment,
        times: impl Into<Times>,
    ) {
        let path_for_attachment = format!("api/mail/v4/attachments/{}/metadata", attachment.id);
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(GetAttachmentMetadataResponse { attachment }),
            )
            .expect(times)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
    /// Generate new mock for retrieving attachment content.
    ///
    /// This function will mock the response for the attachment content request
    /// for the given `attachment_id`.
    ///
    #[function_name::named]
    pub async fn mock_get_attachment_data(
        &self,
        attachment_id: AttachmentId,
        attachment_content: Vec<u8>,
        times: impl Into<Times>,
    ) {
        let path_for_attachment = format!("api/mail/v4/attachments/{attachment_id}");
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(attachment_content))
            .expect(times)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving attachment content that may or may not happen.
    ///
    /// This function will mock the response for the attachment content request
    /// for the given `attachment_id`.
    ///
    #[function_name::named]
    pub async fn mock_maybe_get_attachment_data(
        &self,
        attachment_id: AttachmentId,
        attachment_content: Vec<u8>,
    ) {
        let path_for_attachment = format!("api/mail/v4/attachments/{attachment_id}");
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(attachment_content))
            .up_to_n_times(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for deleting an attachment on the server.
    ///
    #[function_name::named]
    pub async fn mock_delete_attachment(&self, attachment_id: AttachmentId) {
        let path_for_attachment = format!("api/mail/v4/attachments/{attachment_id}");
        Mock::given(method("DELETE"))
            .and(path(path_for_attachment))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for creating a new attachment.
    ///
    /// Note that encrypted parts of the data are not checked as they are prone to change
    /// on every run. We only validate that the corresponding part is there.
    ///
    #[function_name::named]
    pub async fn mock_create_attachment(
        &self,
        params: NewAttachmentParams,
        result: Result<PostAttachmentResponse, (u16, ApiErrorInfo)>,
    ) {
        use wiremock_multipart::matchers::ContainsPart;
        let path_for_attachment = "api/mail/v4/attachments";
        let mut mock = Mock::given(method("POST"))
            .and(path(path_for_attachment))
            .and(
                ContainsPart::new()
                    .with_name("Filename")
                    .with_body(params.filename.into_bytes()),
            )
            .and(
                ContainsPart::new()
                    .with_name("MessageID")
                    .with_body(params.message_id.into_inner().into_bytes()),
            )
            .and(
                ContainsPart::new()
                    .with_name("MIMEType")
                    .with_body(params.mime_type.into_bytes()),
            )
            .and(ContainsPart::new().with_name("DataPacket"))
            .and(ContainsPart::new().with_name("KeyPackets"))
            .and(ContainsPart::new().with_name("Disposition").with_body(
                match &params.disposition {
                    NewAttachmentDisposition::Attachment => "attachment".as_bytes(),
                    NewAttachmentDisposition::Inline(_) => "inline".as_bytes(),
                },
            ));
        if let NewAttachmentDisposition::Inline(cid) = params.disposition {
            mock = mock.and(
                ContainsPart::new()
                    .with_name("ContentID")
                    .with_body(cid.into_bytes()),
            );
        }
        if params.signature.is_some() {
            mock = mock.and(ContainsPart::new().with_name("Signature"));
        }
        if params.enc_signature.is_some() {
            mock = mock.and(ContainsPart::new().with_name("EncSignature"));
        }

        match result {
            Ok(response) => mock.respond_with(ResponseTemplate::new(200).set_body_json(response)),
            Err((code, error)) => {
                mock.respond_with(ResponseTemplate::new(code).set_body_json(error))
            }
        }
        .expect(1)
        .named(function_name!())
        .mount(self.mock_server())
        .await;
    }

    #[function_name::named]
    pub async fn mock_put_attachment_disposition(
        &self,
        attachment_id: AttachmentId,
        new_attachment_disposition: NewAttachmentDisposition,
        response: Result<(), ApiErrorInfo>,
    ) {
        let req = PutAttachmentDispositionRequest::from(new_attachment_disposition);
        let mock = Mock::given(method("PUT"))
            .and(path(format!(
                "api/mail/v4/attachments/{attachment_id}/disposition"
            )))
            .and(body_json(&req));

        match response {
            Ok(_) => mock.respond_with(ResponseTemplate::new(200)),
            Err(err) => mock.respond_with(ResponseTemplate::new(422).set_body_json(err)),
        }
        .expect(1)
        .named(function_name!())
        .mount(self.mock_server())
        .await;
    }
}
