use crate::db::{LocalConversationId, LocalMessageId};
use crate::new_u64_type;
use proton_api_mail::domain::{AttachmentId, AttachmentMetadata, Disposition, MessageAddress};
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::proton_api_core::domain::AddressId;
use proton_crypto_inbox::attachment::{
    self, AttachmentDecryption, AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
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

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(crate = "self::serde")]
pub struct LocalAttachment {
    pub id: LocalAttachmentId,
    pub rid: Option<AttachmentId>,
    pub name: String,
    pub size: u64,
    pub mime_type: String,
    pub address_id: AddressId,
    pub disposition: Disposition,
    pub sender: Option<MessageAddress>,
    // This type is optional as it is possible we did not yet load the messages
    // for a given conversation.
    pub message_id: Option<LocalMessageId>,
    pub conversation_id: Option<LocalConversationId>,
    pub key_packets: attachment::KeyPackets,
    pub signature: Option<attachment::AttachmentSignature>,
    pub encrypted_signature: Option<attachment::AttachmentEncryptedSignature>,
}

impl LocalAttachment {
    #[cfg(test)]
    pub fn from_attachment(
        id: LocalAttachmentId,
        conv_id: LocalConversationId,
        msg_id: Option<LocalMessageId>,
        attachment: &proton_api_mail::domain::Attachment,
    ) -> Self {
        Self {
            id,
            rid: Some(attachment.id.clone()),
            name: attachment.name.clone(),
            size: attachment.size,
            mime_type: attachment.mime_type.clone(),
            address_id: attachment.address_id.clone(),
            disposition: attachment.disposition,
            sender: attachment.sender.clone(),
            message_id: msg_id,
            conversation_id: Some(conv_id),
            key_packets: attachment.key_packets.clone(),
            signature: attachment.signature.clone(),
            encrypted_signature: attachment.enc_signature.clone(),
        }
    }
}

impl From<LocalAttachment> for LocalAttachmentMetadata {
    fn from(value: LocalAttachment) -> Self {
        Self {
            id: value.id,
            rid: value.rid,
            name: value.name,
            size: value.size,
            mime_type: value.mime_type,
            disposition: value.disposition,
        }
    }
}

impl AttachmentDecryption for LocalAttachment {
    fn attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn attachment_signature(&self) -> &Option<AttachmentSignature> {
        &self.signature
    }

    fn attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature> {
        &self.encrypted_signature
    }
}
