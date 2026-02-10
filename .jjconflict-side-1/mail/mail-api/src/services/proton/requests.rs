//! Request structures for the Proton Mail API.
//!
//! This module provides structures that are used to make requests to the Proton
//! Mail API. These structures are used to define the request bodies and URL
//! parameters that are sent to the API when making a request.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint request
//! definitions, and NOT have any business logic or other functionality.
//!
//! Structs in this module should only implement [`Serialize`], and should not
//! implement [`Deserialize`](serde::Deserialize). If anything in this module
//! implements [`Deserialize`](serde::Deserialize), it is a sign that a mistake
//! has been made.
//!
//! Any types that are children of the primary request structures should be
//! defined separately in the [`request_data`](crate::services::proton::request_data)
//! module, or in the [`common`](crate::services::proton::common) module if they
//! used by both requests and responses.
//!

use super::prelude::DirectParams;
use super::request_data::{NewAttachmentDisposition, Package};
use crate::MAX_PAGE_ELEMENT_COUNT_U64;
use crate::services::proton::common::{ConversationId, MessageId};
use crate::services::proton::prelude::{Disposition, NewAttachmentParams};
use crate::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, MessageMetadataSortMode,
};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_crypto_inbox::attachment::{
    BinaryAttachmentEncryptedSignature, BinaryAttachmentSignature,
};
use serde::Serialize;
use serde_with::{BoolFromInt, DisplayFromStr, serde_as};
use smart_default::SmartDefault;

/// Parameters to filter/search conversations with a given criteria.
#[serde_as]
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsOptions {
    #[serde(rename = "AddressID")]
    /// Address ID to filter on.
    pub address_id: Option<AddressId>,

    /// If `true`, only return conversations which have attachments. If `false`,
    /// only return conversations which have no attachments.
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// UNIX timestamp to filter conversations earlier than timestamp.
    pub begin: Option<u64>,

    /// Return only conversations newer, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `begin`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    #[serde(rename = "BeginID")]
    pub begin_id: Option<ConversationId>,

    /// If `true`, return results in descending order rather than ascending.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub desc: Option<bool>,

    /// UNIX timestamp to filter conversations later than timestamp.
    pub end: Option<u64>,

    /// Return only conversations older, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `end`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    #[serde(rename = "EndID")]
    pub end_id: Option<ConversationId>,

    /// Value to filter on according to Sort parameter
    pub anchor: Option<u64>,

    /// Conversation ID use to disambiguate filtering done according to the Anchor parameter
    #[serde(rename = "AnchorID")]
    pub anchor_id: Option<ConversationId>,

    /// External ID to filter on.
    // TODO: Document this properly.
    pub external_id: Option<String>,

    /// Keyword search of `From` field.
    pub from: Option<String>,

    /// Conversation IDs to filter on.
    #[serde(rename = "ID")]
    pub ids: Option<Vec<ConversationId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label ID to filter on.
    #[serde(rename = "LabelID")]
    pub label_id: Option<LabelId>,

    /// The number of conversations to return.
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Sort the results by one of the sorting modes.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search of `Subject` field.
    pub subject: Option<String>,

    /// If `true`, only return conversations which have unread messages. If
    /// `false`, only return conversations which have all messages read.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub unread: Option<bool>,
}

/// Parameters to filter/search messages with a given criteria.
#[serde_as]
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesOptions {
    /// Filter on address ID.
    #[serde(rename = "AddressID")]
    pub address_id: Option<AddressId>,

    /// If `true`, return only messages which have attachments. If `false`,
    /// return only messages which have no attachments.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// Keyword search of `BCC` field.
    #[serde(rename = "BCC")]
    pub bcc: Option<String>,

    /// UNIX timestamp to filter messages at or later than timestamp.
    pub begin: Option<u64>,

    /// Return only messages newer, in creation time (NOT timestamp), than
    /// the specified message ID.
    #[serde(rename = "BeginID")]
    pub begin_id: Option<MessageId>,

    /// Keyword search of CC field.
    #[serde(rename = "CC")]
    pub cc: Option<String>,

    /// Filter messages by conversation ID.
    #[serde(rename = "ConversationID")]
    pub conversation_id: Option<Vec<ConversationId>>,

    /// If `true`, sort results descending. If `false`, sort ascending.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub desc: Option<bool>,

    /// UNIX timestamp to filter messages at or earlier than timestamp.
    pub end: Option<u64>,

    /// Return only messages older, in creation time (NOT timestamp), than the
    /// specified message ID.
    #[serde(rename = "EndID")]
    pub end_id: Option<MessageId>,

    /// Return only messages with the specified anchor.
    pub anchor: Option<u64>,

    /// Return only messages with the specified anchor ID.
    #[serde(rename = "AnchorID")]
    pub anchor_id: Option<MessageId>,

    /// Filter on external ID.
    // TODO: Document this properly.
    #[serde(rename = "ExternalID")]
    pub external_id: Option<String>,

    /// Keyword search `From` field.
    pub from: Option<String>,

    /// Filter on the given message IDs.
    #[serde(rename = "ID")]
    pub ids: Option<Vec<MessageId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label IDs to filter on.
    #[serde(rename = "LabelID")]
    pub label_id: Option<Vec<LabelId>>,

    /// The number of messages to return.
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Result sort mode.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search `Subject` field.
    pub subject: Option<String>,

    /// Keyword search of `To` field.
    pub to: Option<String>,

    /// If `true`, return only messages which are unread. If `false`, return
    /// only messages which are read.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub unread: Option<bool>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsDeleteRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsLabelRequest {
    /// TODO: Document this field.
    pub action: u32,

    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,

    /// TODO: Document this field.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub spam_action: Option<bool>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsReadRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnlabelRequest {
    /// The ids of the conversations to unlabel
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,

    /// The label for the request
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnreadRequest {
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,

    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
}

/// Request to snooze conversations.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsSnoozeRequest {
    /// The ids of the conversations to snooze
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,

    /// The timestamp to snooze until
    pub snooze_time: u64,
}

/// Request to unsnooze conversations.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnsnoozeRequest {
    /// The ids of the conversations to unsnooze
    #[serde(rename = "IDs")]
    pub ids: Vec<ConversationId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesDeleteRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<MessageId>,

    /// TODO: Document this field.
    #[serde(rename = "CurrentLabelID")]
    pub label_id: Option<LabelId>,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesLabelRequest {
    /// TODO: Document this field.
    pub action: u32,

    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<MessageId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,

    /// TODO: Document this field.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub spam_action: Option<bool>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesReadRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<MessageId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnlabelRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<MessageId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnreadRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<MessageId>,
}

/// Request to relabel a message.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostMessagesRelabelRequest {
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,
}

/// Create a new message/draft.
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostCreateDraftRequest {
    ///  Message details.
    pub message: DraftParams,

    /// Draft action used for the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<DraftAction>,

    /// Map of attachment id to encoded key packet.
    pub attachment_key_packets: DraftAttachmentKeyPackets,

    /// Parent message id.
    #[serde(rename = "ParentID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<MessageId>,
}

/// Create a new message/draft.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutUpdateDraftRequest {
    ///  Message details.
    pub message: DraftParams,

    /// Map of attachment id to encoded key packet.
    pub attachment_key_packets: DraftAttachmentKeyPackets,
}

/// Send email request.
/// TODO: Add types for unix timestamps
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostSendRequest {
    /// TODO: document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_time: Option<u64>,

    /// TODO: document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// Indicates if contacts should be automatically created for the recipients.
    #[serde_as(as = "Option<BoolFromInt>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_save_contacts: Option<bool>,

    /// Amount of seconds to delay the message delivery. 0 or absent means delivery now.
    /// If this option is used the message will be considered an undoable send.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_seconds: Option<u64>,

    /// Date when the message has to be delivered. It takes precedence over `DelaySeconds`.
    /// If this option is used, the message will be marked as schedule send.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_time: Option<u64>,

    /// The packages that contain the encrypted emails.
    pub packages: Vec<Package>,
}

#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostSendDirectRequest {
    pub message: DirectParams,
    #[serde(rename = "ParentID")]
    pub parent_id: Option<MessageId>,
    pub action: Option<DraftAction>,
    pub packages: Vec<Package>,
    #[serde_as(as = "BoolFromInt")]
    pub auto_save_contacts: bool,
}

/// Create a new attachment request.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostUploadAttachmentRequest {
    /// File name of the attachment.
    pub filename: String,
    /// Message to which this attachment belongs to.
    #[serde(rename = "MessageID")]
    pub message_id: MessageId,
    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    /// Attachment disposition.
    pub disposition: Disposition,
    /// If disposition is inline, the content id must have value.
    #[serde(rename = "ContentID")]
    pub content_id: Option<String>,
    /// Binary asymmetric key packet.
    pub key_packets: Vec<u8>,
    /// Optional binary detached signature
    pub signature: Option<BinaryAttachmentSignature>,
    /// Optional binary encrypted message containing a binary detached signature.
    pub enc_signature: Option<BinaryAttachmentEncryptedSignature>,
    /// Encrypted attachment payload.
    pub data_packet: Vec<u8>,
}

impl From<NewAttachmentParams> for PostUploadAttachmentRequest {
    fn from(params: NewAttachmentParams) -> Self {
        let (disposition, content_id) = match params.disposition {
            NewAttachmentDisposition::Attachment => (Disposition::Attachment, None),
            NewAttachmentDisposition::Inline(content_id) => (Disposition::Inline, Some(content_id)),
        };
        Self {
            filename: params.filename,
            message_id: params.message_id,
            mime_type: params.mime_type,
            disposition,
            content_id,
            key_packets: params.key_packets,
            signature: params.signature,
            enc_signature: params.enc_signature,
            data_packet: params.data_packet,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutAttachmentDispositionRequest {
    pub disposition: Disposition,
    #[serde(rename = "ContentID")]
    pub content_id: Option<String>,
}

impl From<NewAttachmentDisposition> for PutAttachmentDispositionRequest {
    fn from(value: NewAttachmentDisposition) -> Self {
        match value {
            NewAttachmentDisposition::Attachment => Self {
                disposition: Disposition::Attachment,
                content_id: None,
            },
            NewAttachmentDisposition::Inline(id) => Self {
                disposition: Disposition::Inline,
                content_id: Some(id),
            },
        }
    }
}
