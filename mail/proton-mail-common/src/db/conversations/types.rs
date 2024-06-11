use crate::avatar::AvatarInformation;
use crate::db::{LabelColor, LocalAttachmentMetadata, LocalLabelId};
use crate::exports::serde_json;
use crate::new_u64_type;
use proton_api_mail::domain::{
    Conversation, ConversationId, ExternalId, Label, LabelId, MessageAddress, MessageFlags,
    MessageId, MessageMetadata, MimeType,
};
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::proton_api_core::domain::AddressId;
use std::collections::HashMap;

new_u64_type!(LocalConversationId);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalConversationCount {
    pub id: LocalLabelId,
    pub total: u64,
    pub unread: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalMessageCount {
    pub id: LocalLabelId,
    pub total: u64,
    pub unread: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocalConversation {
    pub id: LocalConversationId,
    pub remote_id: Option<ConversationId>,
    pub order: u64,
    pub subject: String,
    pub senders: Vec<MessageAddress>,
    pub recipients: Vec<MessageAddress>,
    pub num_messages: u64,
    pub num_messages_ctx: u64,
    pub num_unread: u64,
    pub num_attachments: u64,
    pub expiration_time: u64,
    pub snooze_time: u64,
    pub size: u64,
    pub time: u64,
    pub labels: Option<Vec<LocalInlineLabelInfo>>,
    pub starred: bool,
    pub attachments: Option<Vec<LocalAttachmentMetadata>>,
    pub avatar_information: AvatarInformation,
}

impl LocalConversation {
    pub fn from_conversation(
        id: LocalConversationId,
        conversation: Conversation,
        labels: Option<Vec<LocalInlineLabelInfo>>,
    ) -> Self {
        let avatar_information = AvatarInformation::from_message_addresses(&conversation.senders);

        Self {
            id,
            starred: conversation.is_starred(),
            remote_id: Some(conversation.id),
            order: conversation.order,
            subject: conversation.subject,
            senders: conversation.senders,
            recipients: conversation.recipients,
            num_messages: conversation.num_messages,
            num_messages_ctx: 0,
            num_unread: conversation.num_unread,
            num_attachments: conversation.num_attachments,
            expiration_time: conversation.expiration_time,
            size: conversation.size,
            time: 0,
            snooze_time: 0,
            labels,
            attachments: None,
            avatar_information,
        }
    }

    pub fn from_conversation_and_label(
        id: LocalConversationId,
        label_id: &LabelId,
        conversation: Conversation,
        labels: Option<Vec<LocalInlineLabelInfo>>,
    ) -> Self {
        let avatar_information = AvatarInformation::from_message_addresses(&conversation.senders);

        let mut result = Self {
            id,
            starred: conversation.is_starred(),
            remote_id: Some(conversation.id),
            order: conversation.order,
            subject: conversation.subject,
            senders: conversation.senders,
            recipients: conversation.recipients,
            num_messages: conversation.num_messages,
            num_messages_ctx: 0,
            num_unread: conversation.num_unread,
            num_attachments: conversation.num_attachments,
            expiration_time: conversation.expiration_time,
            size: conversation.size,
            labels,
            time: 0,
            snooze_time: 0,
            attachments: None,
            avatar_information,
        };

        if let Some(l) = conversation.labels.iter().find(|l| l.id == *label_id) {
            result.num_unread = l.context_num_unread;
            result.num_messages_ctx = l.context_num_messages;
            result.size = l.context_size;
            result.time = l.context_time;
            result.num_attachments = l.context_num_attachments;
            result.expiration_time = l.context_expiration_time;
            result.snooze_time = l.context_snooze_time;
        }

        result
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[serde(crate = "self::serde")]
pub struct LocalInlineLabelInfo {
    pub id: LocalLabelId,
    pub name: String,
    pub color: LabelColor,
}

impl LocalInlineLabelInfo {
    pub fn from_label(id: LocalLabelId, label: &Label) -> Self {
        Self {
            id,
            name: label.name.clone(),
            color: LabelColor::from(label.color.clone()),
        }
    }
}

new_u64_type!(LocalMessageId);

/// Contains all the metadata associated with a message.
///
/// For the message body see [`LocalMessageBodyMetadata`].
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocalMessageMetadata {
    pub id: LocalMessageId,
    pub rid: Option<MessageId>,
    pub conversation_id: LocalConversationId,
    pub address_id: AddressId,
    pub order: u64,
    pub subject: String,
    pub unread: bool,
    pub sender: MessageAddress,
    pub to: Vec<MessageAddress>,
    pub cc: Vec<MessageAddress>,
    pub bcc: Vec<MessageAddress>,
    pub time: u64,
    pub size: u64,
    pub expiration_time: u64,
    pub snooze_time: u64,
    pub is_replied: bool,
    pub is_replied_all: bool,
    pub is_forwarded: bool,
    pub external_id: Option<ExternalId>,
    pub num_attachments: u32,
    pub flags: MessageFlags,
    pub starred: bool,
    pub attachments: Option<Vec<LocalAttachmentMetadata>>,
    pub labels: Option<Vec<LocalInlineLabelInfo>>,
    pub avatar_information: AvatarInformation,
}

impl LocalMessageMetadata {
    pub fn from_message_metadata(
        id: LocalMessageId,
        conv_id: LocalConversationId,
        message: MessageMetadata,
        labels: Option<Vec<LocalInlineLabelInfo>>,
    ) -> Self {
        let avatar_information = AvatarInformation::from_message_address(&message.sender);
        Self {
            id,
            rid: Some(message.id),
            address_id: message.address_id,
            conversation_id: conv_id,
            order: message.order,
            subject: message.subject,
            unread: message.unread,
            sender: message.sender,
            to: message.to_list,
            cc: message.cc_list,
            bcc: message.bcc_list,
            time: message.time,
            size: message.size,
            expiration_time: message.expiration_time,
            is_replied: message.is_replied,
            is_replied_all: message.is_replied_all,
            is_forwarded: message.is_forwarded,
            external_id: message.external_id,
            num_attachments: message.num_attachments,
            flags: message.flags,
            starred: message.label_ids.contains(LabelId::starred()),
            snooze_time: message.snooze_time,
            attachments: None,
            labels,
            avatar_information,
        }
    }
}

/// Metadata associated with the Body of a message.
///
/// Message bodies are not stored in the database.
///
/// For metadata associated with a message see [`LocalMessageMetadata`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalMessageBodyMetadata {
    pub id: LocalMessageId,
    pub rid: Option<MessageId>,
    pub header: String,
    pub parsed_headers: HashMap<String, serde_json::Value>,
    pub mime_type: MimeType,
    pub address_id: AddressId,
}

#[cfg(test)]
impl LocalMessageBodyMetadata {
    pub fn from_message(id: LocalMessageId, message: &proton_api_mail::domain::Message) -> Self {
        Self {
            id,
            rid: Some(message.metadata.id.clone()),
            header: message.header.clone(),
            parsed_headers: message.parsed_headers.clone(),
            mime_type: message.mime_type,
            address_id: message.metadata.address_id.clone(),
        }
    }
}
