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

use crate::services::proton::common::LabelType;
use crate::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, MessageMetadataSortMode,
};
use crate::MAX_PAGE_ELEMENT_COUNT_U64;
use proton_api_core::services::proton::common::RemoteId;
use serde::Serialize;
use serde_with::{serde_as, BoolFromInt, DisplayFromStr};
use smart_default::SmartDefault;

/// Parameters to filter/search conversations with a given criteria.
#[serde_as]
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsOptions {
    #[serde(rename = "AddressID")]
    /// Address ID to filter on.
    pub address_id: Option<RemoteId>,

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
    pub begin_id: Option<RemoteId>,

    /// If `true`, return results in descending order rather than ascending.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub desc: Option<bool>,

    /// UNIX timestamp to filter conversations later than timestamp.
    pub end: Option<u64>,

    /// Return only conversations older, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `end`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    #[serde(rename = "EndID")]
    pub end_id: Option<RemoteId>,

    /// External ID to filter on.
    // TODO: Document this properly.
    pub external_id: Option<String>,

    /// Keyword search of `From` field.
    pub from: Option<String>,

    /// Conversation IDs to filter on.
    #[serde(rename = "ID")]
    pub ids: Option<Vec<RemoteId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label ID to filter on.
    #[serde(rename = "LabelID")]
    pub label_id: Option<RemoteId>,

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

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLabelsOptions {
    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,
}

/// Represents `POST /labels/by-ids` request body.
///
/// Name refers to the fact it actually gets labels by their IDs.
/// But due to the fact GET requests are not supposed to have a body
/// The struct is used with the POST method instead.
///
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLabelsByIdsOptions {
    /// Label IDs to get.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<RemoteId>,
}

/// Parameters to filter/search messages with a given criteria.
#[serde_as]
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesOptions {
    /// Filter on address ID.
    #[serde(rename = "AddressID")]
    pub address_id: Option<RemoteId>,

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
    pub begin_id: Option<RemoteId>,

    /// Keyword search of CC field.
    #[serde(rename = "CC")]
    pub cc: Option<String>,

    /// Filter messages by conversation ID.
    #[serde(rename = "ConversationID")]
    pub conversation_id: Option<RemoteId>,

    /// If `true`, sort results descending. If `false`, sort ascending.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub desc: Option<bool>,

    /// UNIX timestamp to filter messages at or earlier than timestamp.
    pub end: Option<u64>,

    /// Return only messages older, in creation time (NOT timestamp), than the
    /// specified message ID.
    #[serde(rename = "EndID")]
    pub end_id: Option<RemoteId>,

    /// Filter on external ID.
    // TODO: Document this properly.
    #[serde(rename = "ExternalID")]
    pub external_id: Option<String>,

    /// Keyword search `From` field.
    pub from: Option<String>,

    /// Filter on the given message IDs.
    #[serde(rename = "ID")]
    pub ids: Option<Vec<RemoteId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label IDs to filter on.
    #[serde(rename = "LabelID")]
    pub label_id: Option<Vec<RemoteId>>,

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
pub struct PostLabelsRequest {
    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsDeleteRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,
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
    pub ids: Vec<RemoteId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,

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
    pub ids: Vec<RemoteId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnlabelRequest {
    /// The ids of the conversations to unlabel
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,

    /// The label for the request
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnreadRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutLabelRequest {
    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesDeleteRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,

    /// TODO: Document this field.
    #[serde(rename = "CurrentLabelID")]
    pub label_id: Option<RemoteId>,
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
    pub ids: Vec<RemoteId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,

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
    pub ids: Vec<RemoteId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnlabelRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,

    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnreadRequest {
    /// TODO: Document this field.
    #[serde(rename = "IDs")]
    pub ids: Vec<RemoteId>,
}

/// Request to relabel a message.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostMessagesRelabelRequest {
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<RemoteId>,
}

#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PatchLabelRequest {
    #[serde_as(as = "Option<BoolFromInt>")]
    pub expanded: Option<bool>,
    #[serde_as(as = "Option<BoolFromInt>")]
    pub notify: Option<bool>,
}

/// Create a new message/draft.
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostCreateDraftRequest {
    ///  Message details.
    pub message: DraftParams,

    /// Draft action used for the request.
    pub action: DraftAction,

    /// Map of attachment id to encoded key packet.
    pub attachment_key_packets: DraftAttachmentKeyPackets,

    /// Parent message id.
    #[serde(rename = "ParentID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RemoteId>,
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
