use crate::datatypes::SystemLabelId;
use crate::test_utils::test_context::MailTestContext;
use proton_core_api::services::proton::{Label as ApiLabel, LabelId};
use proton_core_api::services::proton::{ProtonIdMarker, common::ApiErrorInfo};
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::prelude::PutConversationsUnreadRequest;
use proton_mail_api::services::proton::requests::{
    PutConversationsLabelRequest, PutConversationsReadRequest, PutConversationsSnoozeRequest,
    PutConversationsUnlabelRequest, PutConversationsUnsnoozeRequest,
};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel as ApiConversationLabel, MessageMetadata,
    OperationResult,
};
use proton_mail_api::services::proton::responses::{
    GetConversationResponse, PutConversationsLabelResponse, PutConversationsReadResponse,
    PutConversationsSnoozeResponse, PutConversationsUnlabelResponse,
    PutConversationsUnsnoozeResponse,
};
use serde_json;
use std::collections::HashSet;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl MailTestContext {
    /// Generate new mock expectations for labeling conversations.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_label_conversation(
        &self,
        label_id: &LabelId,
        ids: Vec<ConversationId>,
        spam_action: Option<bool>,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsLabelRequest {
            action: 1,
            ids: ids.clone(),
            label_id: label_id.clone(),
            spam_action,
        };
        let resp = PutConversationsLabelResponse {
            responses: build_conv_responses(&ids, failed),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/label"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for unlabeling conversations.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_unlabel_conversation(
        &self,
        label_id: &LabelId,
        ids: Vec<ConversationId>,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsUnlabelRequest {
            ids: ids.clone(),
            label_id: label_id.clone(),
        };
        let resp = PutConversationsUnlabelResponse {
            responses: build_conv_responses(&ids, failed),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/unlabel"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for marking conversations as read.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_mark_conversation_read(
        &self,
        ids: Vec<ConversationId>,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsReadRequest { ids: ids.clone() };
        let resp = PutConversationsReadResponse {
            responses: build_conv_responses(&ids, failed),
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/read"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for marking conversations as unread.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_mark_conversation_unread(
        &self,
        ids: Vec<ConversationId>,
        label_id: LabelId,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsUnreadRequest {
            ids: ids.clone(),
            label_id,
        };
        let resp = PutConversationsReadResponse {
            responses: build_conv_responses(&ids, failed),
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/unread"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for snoozing conversations.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_put_conversations_snooze(
        &self,
        ids: Vec<ConversationId>,
        snooze_time: u64,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsSnoozeRequest {
            ids: ids.clone(),
            snooze_time,
        };
        let resp = PutConversationsSnoozeResponse {
            code: 1000,
            responses: build_conv_responses(&ids, failed),
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/snooze"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for unsnoozing conversations.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// conversations.
    ///
    #[function_name::named]
    pub async fn mock_put_conversations_unsnooze(
        &self,
        ids: Vec<ConversationId>,
        failed: Vec<ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = PutConversationsUnsnoozeRequest { ids: ids.clone() };
        let resp: PutConversationsUnsnoozeResponse = serde_json::from_value(serde_json::json!({
            "Code": 1000,
            "Responses": build_conv_responses(&ids, failed)
        }))
        .unwrap();

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/conversations/unsnooze"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for retrieving a `conversation` and associated `messages`'s
    /// metadata.
    ///
    #[function_name::named]
    pub async fn mock_get_conversation(
        &self,
        conversation: ApiConversation,
        messages: Vec<MessageMetadata>,
    ) {
        let resp = GetConversationResponse {
            conversation,
            messages,
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/api/mail/v4/conversations/{}",
                resp.conversation.id
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_image_for_conversation(&self, response: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/images/logo"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}

/// Build a list of conversation responses.
///
/// This function builds a list of conversation responses for the given `ids`
/// and `failed` conversations.
///
fn build_conv_responses<T: ProtonIdMarker>(ids: &[T], failed: Vec<T>) -> Vec<OperationResult<T>> {
    //TODO: ET-151
    const CODE_SUCCESS: u32 = 1000;
    const CODE_FAIL: u32 = 2000;

    let failed: HashSet<T> = HashSet::from_iter(failed);
    ids.iter()
        .map(|id| {
            let code = if failed.contains(id) {
                CODE_FAIL
            } else {
                CODE_SUCCESS
            };
            OperationResult {
                id: id.clone(),
                response: ApiErrorInfo {
                    code,
                    error: None,
                    details: None,
                },
            }
        })
        .collect()
}

pub trait ApiConversationTestUtils {
    fn test_conversation(id: &str, labels: Vec<ApiLabel>) -> ApiConversation;
    fn test_conversation_in_inbox(id: &str, labels: Vec<ApiLabel>) -> ApiConversation;
}

impl ApiConversationTestUtils for ApiConversation {
    fn test_conversation(id: &str, labels: Vec<ApiLabel>) -> ApiConversation {
        let labels = labels
            .into_iter()
            .map(|l| ApiConversationLabel {
                id: l.id,
                context_num_messages: 1,
                ..ApiConversationLabel::test_default()
            })
            .collect();
        ApiConversation {
            id: id.into(),
            num_messages: 1,
            labels,
            ..ApiConversation::test_default()
        }
    }

    fn test_conversation_in_inbox(id: &str, labels: Vec<ApiLabel>) -> ApiConversation {
        let mut r = Self::test_conversation(id, labels);
        r.labels.insert(
            0,
            ApiConversationLabel {
                id: LabelId::inbox(),
                context_num_messages: 1,
                ..ApiConversationLabel::test_default()
            },
        );

        r
    }
}
