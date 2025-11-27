//! Response structures for the Proton Mail API.
//!
//! This module provides structures that are used to receive responses from the
//! Proton Mail API. These structures are used to define the response bodies
//! that are received from the API when making a request.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint response
//! definitions, and NOT have any business logic or other functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for incoming data.
//!
//! Structs in this module should only implement [`Deserialize`], and should not
//! implement [`Serialize`](serde::Serialize). If anything in this module
//! implements [`Serialize`](serde::Serialize), it is a sign that a mistake has
//! been made. The exception here is for testing purposes, e.g. when mocking
//! response data — in which case implementing [`Serialize`](serde::Serialize)
//! conditionally, only in test mode, is advised.
//!
//! Any types that are children of the primary response structures should be
//! defined separately in the [`response_data`](crate::services::proton::response_data)
//! module, or in the [`common`](crate::services::proton::common) module if they
//! are used by both requests and responses.
//!

use crate::services::proton::IncomingDefault;
use crate::services::proton::common::{ConversationId, MessageId};
use crate::services::proton::prelude::NewAttachmentResponse;
use crate::services::proton::response_data::{
    Attachment, Conversation, ConversationCount, MailSettings, Message, MessageCount,
    MessageMetadata, OperationResult, UndoToken,
};
use proton_api_utils::PaginateResponse;
use serde::Deserialize;
#[cfg(feature = "mocks")]
use serde::Serialize;
use serde_with::{BoolFromInt, DefaultOnNull, serde_as};

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetAttachmentMetadataResponse {
    /// TODO: Document this field.
    pub attachment: Attachment,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationResponse {
    /// TODO: Document this field.
    pub conversation: Conversation,

    /// TODO: Document this field.
    pub messages: Vec<MessageMetadata>,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsResponse {
    /// TODO: Document this field.
    pub conversations: Vec<Conversation>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub stale: bool,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsCountResponse {
    /// TODO: Document this field.
    pub counts: Vec<ConversationCount>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessageResponse {
    /// TODO: Document this field.
    pub message: Message,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesResponse {
    /// TODO: Document this field.
    pub messages: Vec<MessageMetadata>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub stale: bool,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesCountResponse {
    /// TODO: Document this field.
    pub counts: Vec<MessageCount>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMailSettingsResponse {
    /// TODO: Document this field.
    pub mail_settings: MailSettings,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsDeleteResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsLabelResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,

    /// TODO: Document this field.
    pub undo_token: Option<UndoToken>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsReadResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnlabelResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,

    /// TODO: Document this field.
    pub undo_token: Option<UndoToken>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnreadResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesDeleteResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesLabelResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,

    /// TODO: Document this field.
    pub undo_token: Option<UndoToken>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesReadResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnlabelResponse {
    /// TODO: Document this field.
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,

    /// TODO: Document this field.
    pub undo_token: Option<UndoToken>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnreadResponse {
    /// TODO: Document this field.
    pub responses: Vec<OperationResult<MessageId>>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize, Default))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessageHamResponse {
    code: i64,
    added_flag: bool,
    edited_incoming_defaults: bool,
    fed_logs: bool,
    edited_labels: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsSnoozeResponse {
    pub code: i64,
    pub responses: Vec<OperationResult<ConversationId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnsnoozeResponse {
    code: i64,
    pub responses: Vec<OperationResult<ConversationId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostMessagesRelabelResponse {
    pub message: MessageMetadata,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostCreateDraftResponse {
    pub message: Message,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutUpdateDraftResponse {
    pub message: Message,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostSendMessageResponse {
    pub delivery_time: u64, // unix timestamp
    pub sent: Message,
    pub conversation: Conversation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostSendDirectMessageResponse {
    pub sent: Message,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostCancelSendResponse {
    pub message: MessageMetadata,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostAttachmentResponse {
    pub attachment: NewAttachmentResponse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetIncomingDefaultResponse {
    pub incoming_defaults: Vec<IncomingDefault>,
    pub total: u64,
    pub global_total: u64,
}
impl PaginateResponse<IncomingDefault> for GetIncomingDefaultResponse {
    #[allow(clippy::misnamed_getters)] // This is not a mistake
    fn total(&self) -> u64 {
        self.global_total
    }

    fn items(self) -> Vec<IncomingDefault> {
        self.incoming_defaults
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostIncomingDefaultResponse {
    pub incoming_default: IncomingDefault,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutIncomingDefaultResponse {
    pub incoming_default: IncomingDefault,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMobileSettingsResponse {
    pub code: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutNextMessageOnMoveResponse {
    pub code: i64,
    pub mail_settings: MailSettings,
}
