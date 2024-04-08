use crate::{
    conversations::initials, conversations::proton_color, new_u64_type, LabelColor,
    LocalAttachmentMetadata, LocalLabelId,
};
use proton_api_mail::domain::{
    AddressId, Conversation, ConversationId, ExternalId, LabelId, Message, MessageAddress,
    MessageId, MessageMetadata,
};
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};

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
pub struct ConversationAvatarInformation {
    pub text: String,
    pub colour: String,
    pub sender_image_url: String,
}

/// ConversationAvatarInformation contains the details used for the avatar shown for a conversation.
///
/// It contains:
///     - the text to display in the avatar,
///     - the colour to use for the avatar,
///     - and the url of the sender image if a valid BIMI image is available.
impl ConversationAvatarInformation {
    /// build takes a display name and email address and uses these to determine the text and colour the avatar should be
    pub fn build(display_name: &str, email: &str) -> ConversationAvatarInformation {
        ConversationAvatarInformation {
            text: initials::avatar_text(display_name, email),
            colour: proton_color::proton_color(display_name).to_string(),
            sender_image_url: "".to_string(),
        }
    }

    /// from_message_addresses creates a ConversationAvatarInformation struct using the details of the first MessageAddress in the provided slice
    pub fn from_message_addresses(
        address_list: &[MessageAddress],
    ) -> ConversationAvatarInformation {
        let first_sender = address_list.first();
        let display_name_email = match first_sender {
            Some(first_sender) => (first_sender.name.as_str(), first_sender.address.as_str()),
            None => ("", ""),
        };

        ConversationAvatarInformation::build(display_name_email.0, display_name_email.1)
    }
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
    pub size: u64,
    pub time: u64,
    pub labels: Option<Vec<LocalConversationLabel>>,
    pub starred: bool,
    pub attachments: Option<Vec<LocalAttachmentMetadata>>,
    pub avatar_information: ConversationAvatarInformation,
}

impl LocalConversation {
    pub fn from_conversation(
        id: LocalConversationId,
        conversation: Conversation,
        labels: Option<Vec<LocalConversationLabel>>,
    ) -> Self {
        let avatar_information =
            ConversationAvatarInformation::from_message_addresses(&conversation.senders);

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
            labels,
            attachments: None,
            avatar_information,
        }
    }

    pub fn from_conversation_and_label(
        id: LocalConversationId,
        label_id: &LabelId,
        conversation: Conversation,
        labels: Option<Vec<LocalConversationLabel>>,
    ) -> Self {
        let avatar_information =
            ConversationAvatarInformation::from_message_addresses(&conversation.senders);

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
        }

        result
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[serde(crate = "self::serde")]
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
    pub starred: bool,
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
        }
    }
}
