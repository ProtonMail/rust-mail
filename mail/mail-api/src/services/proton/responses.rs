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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetAttachmentMetadataResponse {
    pub attachment: Attachment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationResponse {
    pub conversation: Conversation,
    pub messages: Vec<MessageMetadata>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsResponse {
    pub conversations: Vec<Conversation>,
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub stale: bool,
    pub total: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetConversationsCountResponse {
    pub counts: Vec<ConversationCount>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessageResponse {
    pub message: Message,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesResponse {
    pub messages: Vec<MessageMetadata>,
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub stale: bool,
    pub total: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMessagesCountResponse {
    pub counts: Vec<MessageCount>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetMailSettingsResponse {
    pub mail_settings: MailSettings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsDeleteResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsLabelResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
    pub undo_token: Option<UndoToken>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsReadResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnlabelResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
    pub undo_token: Option<UndoToken>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutConversationsUnreadResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<ConversationId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesDeleteResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesLabelResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
    pub undo_token: Option<UndoToken>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesReadResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnlabelResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<OperationResult<MessageId>>,
    pub undo_token: Option<UndoToken>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutMessagesUnreadResponse {
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
