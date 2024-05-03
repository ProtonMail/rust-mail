use std::io;

use proton_api_mail::{
    domain::{
        Attachment, AttachmentId, AttachmentMetadata, ConversationId, Disposition, MessageId,
    },
    proton_api_core::domain::AddressId,
};
use proton_crypto_inbox::attachment::KeyPackets;

use super::account::TEST_ADDRESS_ID;

const TEST_ATTACHMENT_ID: &str =
    "5OkOlBi3Swa4cHRyChyUazwt8GYDBLIAX-ZYnGg8-nAHNKjj5EgR5uH-GePQFaWQPgS60aoJ1Dl2s6UI4BmwNw==";

/// The metadata for the default attachment.
pub fn test_attachment_metadata() -> AttachmentMetadata {
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
pub fn test_attachment(message_id: MessageId, conversation_id: ConversationId) -> Attachment {
    let metadata = test_attachment_metadata();
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
pub fn test_attachment_data() -> Vec<u8> {
    vec![
        210, 59, 1, 75, 249, 106, 13, 153, 197, 164, 144, 235, 96, 92, 106, 220, 206, 208, 189, 17,
        127, 6, 220, 69, 65, 126, 205, 138, 245, 180, 110, 215, 254, 99, 121, 249, 127, 69, 117,
        44, 194, 232, 202, 197, 150, 65, 245, 172, 8, 130, 69, 101, 144, 170, 20, 17, 58, 171, 52,
        247, 130,
    ]
}

/// The expected plaintext content of the default test attachment.
pub fn test_expected_attachment_decrypted() -> Vec<u8> {
    b"attachment".to_vec()
}
