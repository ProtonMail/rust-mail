use crate::{new_u64_type, LabelColor, LocalLabelId};
use proton_api_mail::domain::{
    AddressId, Conversation, ConversationId, ExternalId, LabelId, MessageAddress, MessageId,
    MessageMetadata,
};

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
    pub num_unread: u64,
    pub num_attachments: u64,
    pub expiration_time: u64,
    pub size: u64,
    pub time: u64,
    pub labels: Option<Vec<LocalConversationLabel>>,
    pub flagged: bool,
}

impl LocalConversation {
    pub fn from_conversation(
        id: LocalConversationId,
        conversation: Conversation,
        labels: Option<Vec<LocalConversationLabel>>,
    ) -> Self {
        Self {
            id,
            remote_id: Some(conversation.id),
            order: conversation.order,
            subject: conversation.subject,
            senders: conversation.senders,
            recipients: conversation.recipients,
            num_messages: conversation.num_messages,
            num_unread: conversation.num_unread,
            num_attachments: conversation.num_attachments,
            expiration_time: conversation.expiration_time,
            size: conversation.size,
            time: 0,
            labels,
            flagged: false,
        }
    }

    pub fn from_conversation_and_label(
        id: LocalConversationId,
        label_id: &LabelId,
        conversation: Conversation,
        labels: Option<Vec<LocalConversationLabel>>,
    ) -> Self {
        let mut result = Self {
            id,
            remote_id: Some(conversation.id),
            order: conversation.order,
            subject: conversation.subject,
            senders: conversation.senders,
            recipients: conversation.recipients,
            num_messages: conversation.num_messages,
            num_unread: conversation.num_unread,
            num_attachments: conversation.num_attachments,
            expiration_time: conversation.expiration_time,
            size: conversation.size,
            labels,
            time: 0,
            flagged: false,
        };

        if let Some(l) = conversation.labels.iter().find(|l| l.id == *label_id) {
            result.num_unread = l.context_num_unread;
            result.num_messages = l.context_num_messages;
            result.size = l.context_size;
            result.time = l.context_time;
            result.num_attachments = l.context_num_attachments;
        }

        result
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocalConversationLabel {
    pub id: LocalLabelId,
    pub name: String,
    pub color: LabelColor,
}

new_u64_type!(LocalMessageId);

#[derive(Debug, Clone, Eq, PartialEq)]
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
    pub is_replied: bool,
    pub is_replied_all: bool,
    pub is_forwarded: bool,
    pub external_id: Option<ExternalId>,
    pub num_attachments: u32,
    pub flags: u64,
}

impl LocalMessageMetadata {
    pub fn from_message_metadata(
        id: LocalMessageId,
        conv_id: LocalConversationId,
        message: MessageMetadata,
    ) -> Self {
        Self {
            id,
            rid: Some(message.id),
            address_id: message.address_id,
            conversation_id: conv_id,
            order: message.order,
            subject: message.subject,
            unread: message.unread.into(),
            sender: message.sender,
            to: message.to_list,
            cc: message.cc_list,
            bcc: message.bcc_list,
            time: message.time,
            size: message.size,
            expiration_time: message.expiration_time,
            is_replied: message.is_replied.into(),
            is_replied_all: message.is_replied_all.into(),
            is_forwarded: message.is_forwarded.into(),
            external_id: message.external_id,
            num_attachments: message.num_attachments,
            flags: message.flags,
        }
    }
}
