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
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub tasks_running: RunningTasks,
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
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub tasks_running: RunningTasks,
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(untagged)]
pub enum RunningTasks {
    /// We don't know if any background tasks are running - this is returned for
    /// requests that don't specify the `?LabelID=...` parameter.
    #[default]
    NotKnown,

    /// No background tasks are running.
    None([(); 0]),

    /// Some background tasks are running.
    ///
    /// The value is a map from label ids onto some metadata describing nature
    /// of the tasks that are running on those labels - since we don't care
    /// about that, we don't model that metadata here.
    Some(serde_json::Value),
}

impl RunningTasks {
    #[must_use]
    pub fn none() -> Self {
        Self::None([])
    }

    #[must_use]
    pub fn some() -> Self {
        Self::Some(serde_json::Value::Object(serde_json::Map::default()))
    }

    #[must_use]
    pub fn is_not_known(&self) -> bool {
        matches!(self, Self::NotKnown)
    }

    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None(_))
    }

    #[must_use]
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Some(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_tasks_deserialization() {
        let parse = |s: &str| -> GetConversationsResponse { serde_json::from_str(s).unwrap() };

        // ---

        let target = parse(
            r#"
            {
              "Conversations": [],
              "Stale": 0,
              "Total": 0
            }
            "#,
        );

        assert_eq!(RunningTasks::NotKnown, target.tasks_running);

        assert!(target.tasks_running.is_not_known());
        assert!(!target.tasks_running.is_none());
        assert!(!target.tasks_running.is_some());

        // ---

        let target = parse(
            r#"
            {
              "Conversations": [],
              "TasksRunning": null,
              "Stale": 0,
              "Total": 0
            }
            "#,
        );

        assert_eq!(RunningTasks::NotKnown, target.tasks_running);

        assert!(target.tasks_running.is_not_known());
        assert!(!target.tasks_running.is_none());
        assert!(!target.tasks_running.is_some());

        // ---

        let target = parse(
            r#"
            {
              "Conversations": [],
              "TasksRunning": [],
              "Stale": 0,
              "Total": 0
            }
            "#,
        );

        assert!(matches!(target.tasks_running, RunningTasks::None(..)));

        assert!(!target.tasks_running.is_not_known());
        assert!(target.tasks_running.is_none());
        assert!(!target.tasks_running.is_some());

        // ---

        let target = parse(
            r#"
            {
              "Conversations": [],
              "TasksRunning": {
                "label-id": 1234
              },
              "Stale": 0,
              "Total": 0
            }
            "#,
        );

        assert!(matches!(target.tasks_running, RunningTasks::Some(..)));

        assert!(!target.tasks_running.is_not_known());
        assert!(!target.tasks_running.is_none());
        assert!(target.tasks_running.is_some());
    }
}
