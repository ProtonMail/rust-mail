use crate::test_context::MailTestContext;
use proton_api_core::services::proton::common::AddressId;
use proton_api_mail::services::proton::common::{AttachmentId, ConversationId, MessageId};
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, AttachmentMetadata as ApiAttachmentMetadata,
    Disposition as ApiDisposition,
};
use proton_api_mail::services::proton::responses::GetAttachmentMetadataResponse;
use proton_core_test_utils::account::TEST_ADDRESS_ID;
use proton_crypto_inbox::attachment::KeyPackets;
use proton_mail_common::datatypes::attachment;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

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
        key_packets: KeyPackets::from("wV4DGS71hsmM2EQSAQdAFwebQBU6CrI3xDOoDnKxPTNV9OiWHh3b+40HFTJckzows6dP/z/dRsZZPKn/Hg4kH7mJTseFFN6yGJlnx22nNzN/+KGYR5Gb+uEaFbpAZFuC"),
        signature: None,
        enc_signature: None,
        sender: None,
        address_id: AddressId::from(TEST_ADDRESS_ID),
        message_id,
        conversation_id,
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
    /// # Parameters
    ///
    /// * `attachment` - The metadata to return as a response.
    ///
    #[function_name::named]
    pub async fn mock_get_attachment_metadata(&self, attachment: ApiAttachment) {
        let path_for_attachment = format!("api/mail/v4/attachments/{}/metadata", attachment.id);
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(GetAttachmentMetadataResponse { attachment }),
            )
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
    /// Generate new mock for retrieving attachment content.
    ///
    /// This function will mock the response for the attachment content request
    /// for the given `attachment_id`.
    ///
    /// # Parameters
    ///
    /// * `attachment_id`      - The attachment id the content should correspond to.
    /// * `attachment_content` - The attachment content the mock replies with.
    ///
    #[function_name::named]
    pub async fn mock_get_attachment_data(
        &self,
        attachment_id: AttachmentId,
        attachment_content: Vec<u8>,
    ) {
        let path_for_attachment = format!("api/mail/v4/attachments/{attachment_id}");
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(attachment_content))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving attachment content that may or may not happen.
    ///
    /// This function will mock the response for the attachment content request
    /// for the given `attachment_id`.
    ///
    /// # Parameters
    ///
    /// * `attachment_id`      - The attachment id the content should correspond to.
    /// * `attachment_content` - The attachment content the mock replies with.
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
}
