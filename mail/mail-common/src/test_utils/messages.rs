use crate::test_utils::test_context::MailTestContext;
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_common::utils::MapVec;
use proton_crypto_inbox::keys::PackageCryptoType;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    AddressSubPackage, AuthInput, IncomingDefault, Package, PostCancelSendResponse,
    PostIncomingDefaultResponse, PostSendDirectMessageResponse, PostSendRequest,
    PutIncomingDefaultResponse, PutMessageHamResponse,
};
use proton_mail_api::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_mail_api::services::proton::requests::{
    PostCreateDraftRequest, PutMessagesDeleteRequest, PutMessagesLabelRequest,
    PutMessagesReadRequest, PutMessagesUnlabelRequest, PutMessagesUnreadRequest,
    PutUpdateDraftRequest,
};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, Message as ApiMessage, MessageMetadata, MimeType,
    OperationResult,
};
use proton_mail_api::services::proton::responses::{
    GetMessageResponse, GetMessagesResponse, PostCreateDraftResponse, PostMessagesRelabelResponse,
    PostSendMessageResponse, PutMessagesDeleteResponse, PutMessagesLabelResponse,
    PutMessagesReadResponse, PutMessagesUnlabelResponse, PutMessagesUnreadResponse,
};
use serde::Serialize;
use serde_json::{Value as JsonValue, json};
use serde_with::{BoolFromInt, serde_as};
use std::collections::{HashMap, HashSet};
use wiremock::matchers::{body_json, body_partial_json, method, path, query_param};
use wiremock::{Mock, ResponseTemplate, Times};

impl MailTestContext {
    #[function_name::named]
    pub async fn mock_get_message_failure(
        &self,
        message_id: &MessageId,
        http_error: u16,
        error: ApiErrorInfo,
    ) {
        Mock::given(method("GET"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(http_error).set_body_json(error))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_get_message(&self, message_id: &MessageId, message: ApiMessage) {
        self.mock_get_message_with_expected(message_id, message, 1)
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_message_with_expected(
        &self,
        message_id: &MessageId,
        message: ApiMessage,
        expected: impl Into<Times>,
    ) {
        let resp = GetMessageResponse { message };

        Mock::given(method("GET"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(expected)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub fn mock_get_messages(&self) -> GetMessagesMock {
        GetMessagesMock::new(self, function_name!())
    }

    #[function_name::named]
    pub async fn mock_label_messages(&self, label_id: &LabelId, message_ids: Vec<MessageId>) {
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_message_delete(
        &self,
        message_ids: impl IntoIterator<Item = MessageId>,
        current_label_id: Option<LabelId>,
        response: PutMessagesDeleteResponse,
    ) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/delete"))
            .and(body_json(PutMessagesDeleteRequest {
                ids: message_ids.into_iter().collect(),
                label_id: current_label_id,
            }))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_empty_label(&self) {
        Mock::given(method("DELETE"))
            .and(path("/api/mail/v4/messages/empty"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesDeleteResponse { responses: vec![] }),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_messages_ok(&self) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/delete"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesDeleteResponse { responses: vec![] }),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/read"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesReadResponse { responses: vec![] }),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;

        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/messages/unread"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(PutMessagesUnreadResponse { responses: vec![] }),
            )
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_put_message_ham(&self, id: &MessageId) {
        Mock::given(method("PUT"))
            .and(path(format!("/api/mail/v4/messages/{id}/mark/ham")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(PutMessageHamResponse::default()),
            )
            .expect(1)
            .named(format!("mock put_message_ham, id = {id}"))
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_put_messages_read(
        &self,
        message_ids: Vec<MessageId>,
        failed_ids: Vec<MessageId>,
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_put_messages_unread(
        &self,
        message_ids: Vec<MessageId>,
        failed_ids: Vec<MessageId>,
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_unlabel_messages(
        &self,
        label_id: &LabelId,
        message_ids: Vec<MessageId>,
        failed: Vec<MessageId>,
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_relabel_message(&self, id: &MessageId, message: MessageMetadata) {
        let response = PostMessagesRelabelResponse { message };
        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages/{id}/relabel")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not validate the body.
    #[function_name::named]
    pub async fn mock_create_draft(
        &self,
        params: DraftParams,
        action: Option<DraftAction>,
        reply: ApiMessage,
        parent_id: Option<MessageId>,
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not validate the body.
    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_create_draft_no_validation(&self, reply: ApiMessage) {
        let response = PostCreateDraftResponse { message: reply };
        Mock::given(method("POST"))
            .and(path("/api/mail/v4/messages".to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not validate the body.
    #[function_name::named]
    pub async fn mock_create_draft_failure(
        &self,
        params: DraftParams,
        action: Option<DraftAction>,
        parent_id: Option<MessageId>,
        attachment_key_packets: DraftAttachmentKeyPackets,
        error_code: u32,
    ) {
        let response = ApiErrorInfo {
            code: error_code,
            error: None,
            details: None,
        };
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
            .respond_with(ResponseTemplate::new(422).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not validate parameters that are cryptographically
    /// generated.
    #[function_name::named]
    pub async fn mock_send_draft(
        &self,
        message_id: MessageId,
        params: TestDraftSendRequest,
        result_message: ApiMessage,
        result_conversation: ApiConversation,
        delivery_time: u64,
    ) {
        let response = PostSendMessageResponse {
            delivery_time,
            sent: result_message,
            conversation: result_conversation,
        };
        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .and(body_partial_json(params))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_send_draft_failure(&self, message_id: MessageId, error: ApiErrorInfo) {
        Mock::given(method("POST"))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(422).set_body_json(error))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Creates a *partial* mock for the direct mail endpoint.
    ///
    /// This endpoint matches only the plain-text metadata (subject, attachment
    /// file names etc.), ignoring the encrypted stuff.
    #[function_name::named]
    pub async fn mock_send_direct(
        &self,
        subject: &str,
        sender: &str,
        recipient: &str,
        attachments: &[&str],
        parent_id: Option<&str>,
        response: PostSendDirectMessageResponse,
    ) {
        let attachments: JsonValue = attachments
            .iter()
            .map(|attachment| {
                json!({
                    "Filename": attachment,
                })
            })
            .collect();

        let parent_id = parent_id
            .map(|id| JsonValue::String(id.into()))
            .unwrap_or_default();

        let request = json!({
            "Message": {
                "Subject": subject,
                "Sender": {
                    "Address": sender,
                },
                "ToList": [
                    {
                        "Address": recipient,
                    }
                ],
                "Attachments": attachments,
            },
            "ParentID": parent_id,
        });

        Mock::given(method("POST"))
            .and(path("/api/mail/v4/messages/send/direct"))
            .and(body_partial_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not valid the draft body.
    #[function_name::named]
    pub async fn mock_update_draft(
        &self,
        message_id: MessageId,
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not valid the draft body.
    #[function_name::named]
    pub async fn mock_update_draft_failure(
        &self,
        message_id: MessageId,
        params: DraftParams,
        attachment_key_packets: DraftAttachmentKeyPackets,
        reply: ApiErrorInfo,
    ) {
        Mock::given(method("PUT"))
            .and(body_partial_json(TestUpdateDraftRequest::from(
                PutUpdateDraftRequest {
                    message: params,
                    attachment_key_packets,
                },
            )))
            .and(path(format!("/api/mail/v4/messages/{message_id}")))
            .respond_with(ResponseTemplate::new(422).set_body_json(reply))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Note that this mock does not validate parameters that are cryptographically
    /// generated.
    #[function_name::named]
    pub async fn mock_undo_send(
        &self,
        message_id: MessageId,
        result: Result<PostCancelSendResponse, ApiErrorInfo>,
    ) {
        let mock = Mock::given(method("POST")).and(path(format!(
            "/api/mail/v4/messages/{message_id}/cancel_send"
        )));
        match result {
            Ok(response) => mock.respond_with(ResponseTemplate::new(200).set_body_json(response)),
            Err(e) => mock.respond_with(ResponseTemplate::new(422).set_body_json(e)),
        }
        .expect(1)
        .named(function_name!())
        .mount(self.mock_server())
        .await;
    }

    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_delete_incoming_default(&self) {
        Mock::given(method("PUT"))
            .and(path("/api/mail/v4/incomingdefaults/delete"))
            .respond_with(ResponseTemplate::new(200).set_body_json(()))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_post_incoming_default(&self, incoming_default: IncomingDefault) {
        let resp = PostIncomingDefaultResponse { incoming_default };
        Mock::given(method("POST"))
            .and(path("/api/mail/v4/incomingdefaults"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_put_incoming_default(&self, incoming_default: IncomingDefault) {
        let id = incoming_default.id.clone();
        let resp = PutIncomingDefaultResponse { incoming_default };
        Mock::given(method("PUT"))
            .and(path(format!("/api/mail/v4/incomingdefaults/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_post_incoming_default_n(
        &self,
        incoming_default: IncomingDefault,
        times: u64,
    ) {
        let resp = PostIncomingDefaultResponse { incoming_default };
        Mock::given(method("POST"))
            .and(path("/api/mail/v4/incomingdefaults"))
            .respond_with(ResponseTemplate::new(200).set_body_json(resp))
            .expect(times)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[allow(clippy::doc_markdown)]
    #[function_name::named]
    pub async fn mock_report_phishing(&self) {
        Mock::given(method("POST"))
            .and(path("/api/core/v4/reports/phishing"))
            .respond_with(ResponseTemplate::new(200).set_body_json(()))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}

/// Build a list of message responses.
///
/// This function builds a list of message responses for the given `ids`
/// and `failed` messages.
///
fn build_message_responses(
    ids: &[MessageId],
    failed: Vec<MessageId>,
) -> Vec<OperationResult<MessageId>> {
    const CODE_SUCCESS: u32 = 1000;
    const CODE_FAIL: u32 = 2000;

    let failed: HashSet<MessageId> = HashSet::from_iter(failed);
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
    #[serde(rename = "CCList")]
    pub cc_list: Vec<DraftRecipient>,
    #[serde(rename = "BCCList")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<DraftAction>,
    pub attachment_key_packets: DraftAttachmentKeyPackets,
    #[serde(rename = "ParentID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<MessageId>,
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

/// We can't use the full request struct to mock as it contains data that is cryptographically
/// generated. So we use a partial completion approach.
#[serde_as]
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestDraftSendRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    #[serde_as(as = "Option<BoolFromInt>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_save_contacts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_time: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<TestDraftSendPackage>,
}

impl From<PostSendRequest> for TestDraftSendRequest {
    fn from(value: PostSendRequest) -> Self {
        Self {
            expiration_time: value.expiration_time,
            expires_in: value.expires_in,
            auto_save_contacts: value.auto_save_contacts,
            delay_seconds: value.delay_seconds,
            delivery_time: value.delivery_time,
            packages: value.packages.into_iter().map_vec(),
        }
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestDraftSendPackage {
    pub addresses: HashMap<String, TestDraftSendAddressSubPackage>,
}

impl From<Package> for TestDraftSendPackage {
    fn from(value: Package) -> Self {
        Self {
            addresses: value
                .addresses
                .into_iter()
                .map(|(key, v)| (key, v.into()))
                .collect(),
        }
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestDraftSendAddressSubPackage {
    #[serde(rename = "Type")]
    pub address_type: PackageCryptoType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<TestDraftAuthInput>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestDraftAuthInput {
    #[serde(rename = "ModulusID")]
    pub modulus_id: String,
}

impl From<AuthInput> for TestDraftAuthInput {
    fn from(value: AuthInput) -> Self {
        Self {
            modulus_id: value.modulus_id,
        }
    }
}

impl From<AddressSubPackage> for TestDraftSendAddressSubPackage {
    fn from(value: AddressSubPackage) -> Self {
        Self {
            address_type: value.address_type,
            auth: value.auth.map(Into::into),
        }
    }
}

pub struct GetMessagesMock<'a> {
    ctx: &'a MailTestContext,
    name: &'static str,
    label_id: Option<String>,
    keyword: Option<String>,
    end_id: Option<String>,
    expect: Option<Times>,
}

impl<'a> GetMessagesMock<'a> {
    fn new(ctx: &'a MailTestContext, name: &'static str) -> Self {
        Self {
            ctx,
            name,
            label_id: None,
            keyword: None,
            end_id: None,
            expect: None,
        }
    }

    pub fn given_label_id(mut self, label_id: &LabelId) -> Self {
        self.label_id = Some(label_id.to_string());
        self
    }

    pub fn given_keyword(mut self, keyword: &str) -> Self {
        self.keyword = Some(keyword.into());
        self
    }

    pub fn given_end_id(mut self, end_id: &str) -> Self {
        self.end_id = Some(end_id.into());
        self
    }

    pub fn expect(mut self, expect: impl Into<Times>) -> Self {
        self.expect = Some(expect.into());
        self
    }

    pub async fn respond_with(self, messages: Vec<MessageMetadata>) {
        self.respond_with_ex(messages.len(), messages).await;
    }

    pub async fn respond_with_ex(self, total: usize, messages: Vec<MessageMetadata>) {
        let mut mock = Mock::given(method("GET")).and(path("/api/mail/v4/messages"));

        if let Some(label_id) = self.label_id {
            mock = mock.and(query_param("LabelID[0]", label_id.to_string()));
        }

        if let Some(end_id) = self.end_id {
            mock = mock.and(query_param("EndID", end_id));
        }

        if let Some(keyword) = self.keyword {
            mock = mock.and(query_param("Keyword", keyword));
        }

        mock.respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                total: total.try_into().unwrap(),
                messages,
                stale: false,
            }),
        )
        .expect(self.expect.unwrap_or_else(|| 1.into()))
        .named(self.name)
        .mount(self.ctx.mock_server())
        .await;
    }
}
