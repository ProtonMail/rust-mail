use crate::test_context::MailTestContext;
use itertools::Itertools;
use proton_api_core::services::proton::common::{RemoteId as ApiRemoteId, RemoteId};
use proton_api_core::services::proton::response_data::ApiErrorInfo;
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::requests::{
    PostCreateDraftRequest, PutMessagesLabelRequest, PutMessagesReadRequest,
    PutMessagesUnlabelRequest, PutMessagesUnreadRequest, PutUpdateDraftRequest,
};
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, Message as ApiMessage, MessageMetadata, MimeType,
    OperationResult,
};
use proton_api_mail::services::proton::responses::{
    GetMessageResponse, GetMessagesResponse, PostCreateDraftResponse, PostMessagesRelabelResponse,
    PostSendMessageResponse, PutMessagesDeleteResponse, PutMessagesLabelResponse,
    PutMessagesReadResponse, PutMessagesUnlabelResponse, PutMessagesUnreadResponse,
};
use serde::Serialize;
use serde_with::{serde_as, BoolFromInt};
use std::collections::HashSet;
use wiremock::matchers::{body_json, body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl MailTestContext {
    /// Generate new mock expectations for message fetch request for `message_id`.
    pub async fn mock_get_message(&self, message_id: &ApiRemoteId, message: ApiMessage) {
        self.mock_get_message_with_expected(message_id, message, 1)
            .await;
    }

    /// Generate new mock expectations for message fetch request for `message_id`.
    ///
    /// This mock is expected to be called `expected` number of times.
    pub async fn mock_get_message_with_expected(
        &self,
        message_id: &ApiRemoteId,
        message: ApiMessage,
        expected: u64,
    ) {
        let resp = GetMessageResponse { message };

        Mock::given(method("GET"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(expected)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectation for batch messages request
    pub async fn mock_get_messages(&self, messages: Vec<MessageMetadata>) {
        let resp = GetMessagesResponse {
            total: messages.len() as u64,
            messages,
            stale: false,
        };

        Mock::given(method("GET"))
            .and(path("/api/mail/v4/messages".to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for labeling messages.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The label ID to use for the request.
    /// * `message_ids` - The list of message IDs to label.
    /// * `spam_action` - The spam action to use for the request.
    /// * `failed`      - The list of message IDs for which we want to
    ///                   simulate failure.
    ///
    pub async fn mock_label_messages(&self, label_id: &ApiRemoteId, message_ids: Vec<ApiRemoteId>) {
        let ids = message_ids.clone();
        let request = PutMessagesLabelRequest {
            action: 1,
            ids: ids.clone(),
            label_id: label_id.clone(),
            spam_action: None,
        };
        let response = PutMessagesLabelResponse {
            responses: build_message_responses(&ids, vec![]),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/label"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_messages_ok(&self) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/delete"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesDeleteResponse { responses: vec![] }),
            )
            .mount(self.mock_server())
            .await;

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/read"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesReadResponse { responses: vec![] }),
            )
            .mount(self.mock_server())
            .await;

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/unread"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesUnreadResponse { responses: vec![] }),
            )
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for marking messages as read.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - The list of message IDs to mark read.
    /// * `failed_ids`  - The list of message IDs for which we want to
    ///                   simulate failure.
    ///
    pub async fn mock_put_messages_read(
        &self,
        message_ids: Vec<ApiRemoteId>,
        failed_ids: Vec<ApiRemoteId>,
    ) {
        let message_ids = message_ids.into_iter().collect_vec();
        let request = PutMessagesReadRequest {
            ids: message_ids.clone(),
        };
        let resp = PutMessagesReadResponse {
            responses: build_message_responses(&message_ids, failed_ids),
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/read"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for marking messages as unread.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - The list of message IDs to mark unread.
    /// * `failed_ids`  - The list of message IDs for which we want to
    ///                   simulate failure.
    pub async fn mock_put_messages_unread(
        &self,
        message_ids: Vec<ApiRemoteId>,
        failed_ids: Vec<ApiRemoteId>,
    ) {
        let message_ids = message_ids.into_iter().collect_vec();
        let request = PutMessagesUnreadRequest {
            ids: message_ids.clone(),
        };
        let resp = PutMessagesUnreadResponse {
            responses: build_message_responses(&message_ids, failed_ids),
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/unread"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for unlabeling messages.
    ///
    /// This function will mock the response for the given `ids` and `failed`
    /// messages.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The label ID to use for the request.
    /// * `message_ids` - The list of message IDs to label.
    /// * `spam_action` - The spam action to use for the request.
    /// * `failed`      - The list of message IDs for which we want to
    ///                   simulate failure.
    ///
    pub async fn mock_unlabel_messages(
        &self,
        label_id: &ApiRemoteId,
        message_ids: Vec<ApiRemoteId>,
        failed: Vec<ApiRemoteId>,
    ) {
        let ids = message_ids.clone();
        let request = PutMessagesUnlabelRequest {
            ids: ids.clone(),
            label_id: label_id.clone(),
        };
        let response = PutMessagesUnlabelResponse {
            responses: build_message_responses(&ids, failed),
            undo_token: None,
        };

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/unlabel"))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock expectations for relabel message.
    ///
    /// # Parameters
    ///
    /// * `id`      - ID of the message to relabel.
    /// * `message` - modified message as response.
    ///
    pub async fn mock_relabel_message(&self, id: &ApiRemoteId, message: MessageMetadata) {
        let response = PostMessagesRelabelResponse { message };
        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages/{id}/relabel")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate a new mock expectation for creating a draft.
    ///
    /// Note that this mock does not valid the draft body.
    ///
    /// # Parameters
    ///
    /// * `params`                 - Expected draft params.
    /// * `action`                 - Draft action (Reply, ReplyAll, Forward)
    /// * `reply`                  - Expected server reply.
    /// * `parent_id`              - Parent id to from which we are
    ///                              replying/forwarding to/from
    /// * `attachment_key_packets` - Attachment key packets for the attachment.
    ///                              included in this request.
    #[allow(clippy::doc_markdown)]
    pub async fn mock_create_draft(
        &self,
        params: DraftParams,
        action: DraftAction,
        reply: ApiMessage,
        parent_id: Option<ApiRemoteId>,
        attachment_key_packets: DraftAttachmentKeyPackets,
    ) {
        let response = PostCreateDraftResponse { message: reply };
        Mock::given(method("POST"))
            .and(body_partial_json(TestCreateDraftRequest::from(
                PostCreateDraftRequest {
                    message: params,
                    action,
                    attachment_key_packets,
                    parent_id,
                },
            )))
            .and(path("/api/mail/v4/messages".to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate a new mock expectation for sending a draft.
    ///
    /// Note that this mock does not validate the parameters, only
    /// that the request was made.
    ///
    /// # Parameters
    ///
    /// * `message_id` - Message to send
    /// * `result_message` - Updated message returned by the API.
    /// * `result_conversation` - Updated conversation returned by API.
    #[allow(clippy::doc_markdown)]
    pub async fn mock_send_draft_basic(
        &self,
        message_id: ApiRemoteId,
        result_message: ApiMessage,
        result_conversation: ApiConversation,
    ) {
        let response = PostSendMessageResponse {
            delivery_time: 0,
            sent: result_message,
            conversation: result_conversation,
        };
        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate a new mock expectation for updating a draft.
    ///
    /// Note that this mock does not valid the draft body.
    ///
    /// # Parameters
    ///
    /// * `message_id`             - Message id to update.
    /// * `params`                 - Expected draft params.
    /// * `reply`                  - Expected server reply.
    /// * `attachment_key_packets` - Attachment key packets for the attachment.
    ///                              included in this request.
    #[allow(clippy::doc_markdown)]
    pub async fn mock_update_draft(
        &self,
        message_id: RemoteId,
        params: DraftParams,
        reply: ApiMessage,
        attachment_key_packets: DraftAttachmentKeyPackets,
    ) {
        let response = PostCreateDraftResponse { message: reply };
        Mock::given(method("PUT"))
            .and(body_partial_json(TestUpdateDraftRequest::from(
                PutUpdateDraftRequest {
                    message: params,
                    attachment_key_packets,
                },
            )))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}

/// Build a list of message responses.
///
/// This function builds a list of message responses for the given `ids`
/// and `failed` messages.
///
/// # Parameters
///
/// * `ids`    - The list of message IDs to build responses for.
/// * `failed` - The list of message IDs for which we want to simulate failure.
///
fn build_message_responses(ids: &[ApiRemoteId], failed: Vec<ApiRemoteId>) -> Vec<OperationResult> {
    const CODE_SUCCESS: u32 = 1000;
    const CODE_FAIL: u32 = 2000;

    let failed: HashSet<ApiRemoteId> = HashSet::from_iter(failed);
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

/// We can't use the regular draft params as the encrypted message
/// changes every time we attempt to create it. So we use version
/// which does not include the body.
///
/// See [`DraftParams`] for more details.
#[serde_as]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestDraftParams {
    pub subject: String,
    #[serde_as(as = "BoolFromInt")]
    pub unread: bool,
    pub sender: DraftSender,
    pub to_list: Vec<DraftRecipient>,
    pub cc_list: Vec<DraftRecipient>,
    pub bcc_list: Vec<DraftRecipient>,
    pub external_id: Option<String>,
    pub draft_flags: u32,
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,
}

impl From<DraftParams> for TestDraftParams {
    fn from(value: DraftParams) -> Self {
        Self {
            subject: value.subject,
            unread: value.unread,
            sender: value.sender,
            to_list: value.to_list,
            cc_list: value.cc_list,
            bcc_list: value.bcc_list,
            external_id: value.external_id,
            draft_flags: value.draft_flags,
            mime_type: value.mime_type,
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
/// We can't use the regular draft params as the encrypted message
/// changes every time we attempt to create it. So we use version
/// which does not include the body.
pub struct TestCreateDraftRequest {
    pub message: TestDraftParams,
    pub action: DraftAction,
    pub attachment_key_packets: DraftAttachmentKeyPackets,
    #[serde(rename = "ParentID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RemoteId>,
}

impl From<PostCreateDraftRequest> for TestCreateDraftRequest {
    fn from(value: PostCreateDraftRequest) -> Self {
        Self {
            message: value.message.into(),
            action: value.action,
            attachment_key_packets: value.attachment_key_packets,
            parent_id: value.parent_id,
        }
    }
}

#[serde_as]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
/// We can't use the regular draft params as the encrypted message
/// changes every time we attempt to create it. So we use version
/// which does not include the body.
pub struct TestUpdateDraftRequest {
    pub message: TestDraftParams,
    pub attachment_key_packets: DraftAttachmentKeyPackets,
}

impl From<PutUpdateDraftRequest> for TestUpdateDraftRequest {
    fn from(value: PutUpdateDraftRequest) -> Self {
        Self {
            message: value.message.into(),
            attachment_key_packets: value.attachment_key_packets,
        }
    }
}
