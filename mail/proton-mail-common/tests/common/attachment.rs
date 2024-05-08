use std::{fmt::format, io};

use proton_api_mail::{
    domain::{
        Attachment, AttachmentId, AttachmentMetadata, ConversationId, Disposition, MessageId,
    },
    proton_api_core::domain::AddressId,
    requests::GetAttachmentMetadataResponse,
};
use proton_crypto_inbox::attachment::KeyPackets;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{account::TEST_ADDRESS_ID, TestContext};

const TEST_ATTACHMENT_ID: &str =
    "5OkOlBi3Swa4cHRyChyUazwt8GYDBLIAX-ZYnGg8-nAHNKjj5EgR5uH-GePQFaWQPgS60aoJ1Dl2s6UI4BmwNw==";

/// The metadata for the default attachment.
pub fn testdata_attachment_metadata() -> AttachmentMetadata {
    AttachmentMetadata {
        id: AttachmentId::from(TEST_ATTACHMENT_ID),
        size: 61,
        name: "attachment.txt".to_owned(),
        mime_type: "text/plain".to_owned(),
        disposition: Disposition::Attachment,
    }
}

/// The complete metadata for the default attachment.
///
/// The attachment is encrypted with the default test account address key.
pub fn testdata_attachment_metadata_complete(
    message_id: MessageId,
    conversation_id: ConversationId,
) -> Attachment {
    let metadata = testdata_attachment_metadata();
    Attachment {
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
pub fn testdata_attachment_data() -> Vec<u8> {
    vec![
        210, 59, 1, 75, 249, 106, 13, 153, 197, 164, 144, 235, 96, 92, 106, 220, 206, 208, 189, 17,
        127, 6, 220, 69, 65, 126, 205, 138, 245, 180, 110, 215, 254, 99, 121, 249, 127, 69, 117,
        44, 194, 232, 202, 197, 150, 65, 245, 172, 8, 130, 69, 101, 144, 170, 20, 17, 58, 171, 52,
        247, 130,
    ]
}

/// The expected plaintext content of the default test attachment.
pub fn testdata_expected_attachment_decrypted() -> Vec<u8> {
    b"attachment".to_vec()
}

impl TestContext {
    /// Generate new mock for retrieving complete attachment metadata.
    ///
    /// This function will mock the response for the give attachment metadata.
    ///
    /// # Parameters
    ///
    /// * `attachment` - The metadata to return as a response.
    ///
    pub async fn mock_get_attachment_metadata(&self, attachment: Attachment) {
        let path_for_attachment = format!("api/mail/v4/attachments/{}/metadata", attachment.id);
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetAttachmentMetadataResponse {
                    attachment: attachment,
                }),
            )
            .expect(1)
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
    pub async fn mock_get_attachment_data(
        &self,
        attachment_id: AttachmentId,
        attachment_content: Vec<u8>,
    ) {
        let path_for_attachment = format!("api/mail/v4/attachments/{}", attachment_id);
        Mock::given(method("GET"))
            .and(path(path_for_attachment))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(attachment_content))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
