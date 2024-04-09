use crate::common::TestContext;
use proton_api_mail::domain::{ConversationId, LabelId};
use proton_api_mail::proton_api_core::APIErrorDesc;
use proton_api_mail::requests::{
    ConversationsResponseObject, LabelConversationRequest, LabelConversationsResponse,
    MarkConversationsReadRequest, UnlabelConversationRequest,
};
use std::collections::HashSet;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    /// Generate new mock requests for labeling conversations. `failed` should contain a list of
    /// conversations for which we want to simulate failure.
    pub fn mock_label_conversation(
        &self,
        label_id: &LabelId,
        ids: impl IntoIterator<Item = ConversationId>,
        spam_action: Option<bool>,
        failed: impl IntoIterator<Item = ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = LabelConversationRequest::new(label_id, spam_action, &ids);

        let resp = LabelConversationsResponse {
            responses: build_conv_responses(&ids, failed),
            undo_token: None,
        };

        self.async_runtime().block_on(async {
            Mock::given(method("PUT"))
                .and(path("/api/mail/v4/conversations/label"))
                .and(body_json(request))
                .respond_with(ResponseTemplate::new(200).set_body_json(resp))
                .expect(1)
                .mount(self.mock_server())
                .await;
        });
    }

    /// Generate new mock requests for unlabeling conversations. `failed` should contain a list of
    /// conversations for which we want to simulate failure.
    pub fn mock_unlabel_conversation(
        &self,
        label_id: &LabelId,
        ids: impl IntoIterator<Item = ConversationId>,
        failed: impl IntoIterator<Item = ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = UnlabelConversationRequest::new(label_id, &ids);
        let resp = LabelConversationsResponse {
            responses: build_conv_responses(&ids, failed),
            undo_token: None,
        };

        self.async_runtime().block_on(async {
            Mock::given(method("PUT"))
                .and(path("/api/mail/v4/conversations/unlabel"))
                .and(body_json(request))
                .respond_with(ResponseTemplate::new(200).set_body_json(resp))
                .expect(1)
                .mount(self.mock_server())
                .await;
        });
    }

    /// Generate new mock requests for marking conversations as read. `failed` should contain a list of
    /// conversations for which we want to simulate failure.
    pub fn mock_mark_conversation_read(
        &self,
        ids: impl IntoIterator<Item = ConversationId>,
        failed: impl IntoIterator<Item = ConversationId>,
    ) {
        let ids = ids.into_iter().collect::<Vec<_>>();
        let request = MarkConversationsReadRequest::new(&ids);

        let resp = LabelConversationsResponse {
            responses: build_conv_responses(&ids, failed),
            undo_token: None,
        };

        self.async_runtime().block_on(async {
            Mock::given(method("PUT"))
                .and(path("/api/mail/v4/conversations/read"))
                .and(body_json(request))
                .respond_with(ResponseTemplate::new(200).set_body_json(resp))
                .expect(1)
                .mount(self.mock_server())
                .await;
        });
    }
}

fn build_conv_responses(
    ids: &[ConversationId],
    failed: impl IntoIterator<Item = ConversationId>,
) -> Vec<ConversationsResponseObject> {
    //TODO: ET-151
    const CODE_SUCCESS: u32 = 1000;
    const CODE_FAIL: u32 = 2000;

    let failed: HashSet<ConversationId> = HashSet::from_iter(failed);
    ids.iter()
        .map(|id| {
            let code = if failed.contains(&id) {
                CODE_FAIL
            } else {
                CODE_SUCCESS
            };
            ConversationsResponseObject {
                id: id.clone(),
                response: APIErrorDesc {
                    code,
                    error: None,
                    details: None,
                },
            }
        })
        .collect::<Vec<_>>()
}
