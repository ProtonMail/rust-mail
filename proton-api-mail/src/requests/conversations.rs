use crate::domain::{
    Conversation, ConversationCount, ConversationFilter, ConversationId, LabelId, MessageMetadata,
};
use crate::{MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http;
use proton_api_core::http::{JsonResponse, Method, RequestData};
use proton_api_core::utils::{bool_from_integer, bool_to_integer, opt_bool_to_integer};

pub struct GetConversationsRequest {
    filter: ConversationFilter,
}

impl GetConversationsRequest {
    #[must_use]
    pub fn new(mut filter: ConversationFilter) -> Self {
        filter.page_size = filter.page_size.max(MAX_PAGE_ELEMENT_COUNT_U64);
        filter.limit = filter.limit.map(|v| v.max(MAX_LIMIT_VALUE_U64));
        Self { filter }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetConversationsResponse {
    pub conversations: Vec<Conversation>,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub stale: bool,
    pub total: u64,
}

impl http::RequestDesc for GetConversationsRequest {
    type Response = JsonResponse<GetConversationsResponse>;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(Method::Get, "mail/v4/conversations")
            .query("Page", &self.filter.page)
            .query("PageSize", &self.filter.page_size);

        if let Some(ids) = &self.filter.ids {
            data = data.query_array("ID", ids);
        }

        if let Some(subject) = &self.filter.subject {
            data = data.query("Subject", subject);
        }

        if let Some(from) = &self.filter.from {
            data = data.query("From", from);
        }

        if let Some(recipients) = &self.filter.recipients {
            data = data.query_array("Recipients", recipients);
        }

        if let Some(keyword) = &self.filter.keyword {
            data = data.query("Keyword", keyword);
        }

        if let Some(addr_id) = &self.filter.address_id {
            data = data.query("AddressID", addr_id);
        }

        if let Some(label_id) = &self.filter.label_id {
            data = data.query("LabelID", label_id);
        }

        if let Some(external_id) = &self.filter.external_id {
            data = data.query("ExternalID", external_id);
        }

        if let Some(end_id) = &self.filter.end_id {
            data = data.query("EndID", end_id);
        }

        if let Some(begin_id) = &self.filter.begin_id {
            data = data.query("BeginID", begin_id);
        }

        if let Some(end) = &self.filter.end {
            data = data.query("End", end);
        }

        if let Some(begin) = &self.filter.begin {
            data = data.query("Begin", begin);
        }

        if let Some(desc) = self.filter.desc {
            data = data.query("Desc", if desc { &1 } else { &0 });
        }

        if let Some(attachments) = self.filter.attachments {
            data = data.query("Attachments", if attachments { &1 } else { &0 });
        }

        if let Some(unread) = self.filter.unread {
            data = data.query("Unread", if unread { &1 } else { &0 });
        }

        if let Some(sort) = &self.filter.sort {
            data = data.query("Sort", sort);
        }

        if let Some(limit) = &self.filter.limit {
            data = data.query("Limit", limit);
        }

        if let Some(wildcard) = self.filter.auto_wildcard {
            data = data.query("AutoWildcard", if wildcard { &1 } else { &0 });
        }

        data
    }
}

pub struct GetConversationRequest<'a> {
    id: &'a ConversationId,
}

impl<'a> GetConversationRequest<'a> {
    #[must_use]
    pub fn new(id: &'a ConversationId) -> Self {
        Self { id }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetConversationResponse {
    pub conversation: Conversation,
    pub messages: Vec<MessageMetadata>,
}

impl<'a> http::RequestDesc for GetConversationRequest<'a> {
    type Response = JsonResponse<GetConversationResponse>;
    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("mail/v4/conversations/{}", self.id))
    }
}

pub struct GetConversationCountsRequest {}
impl http::RequestDesc for GetConversationCountsRequest {
    type Response = JsonResponse<GetConversationCountsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "mail/v4/conversations/count")
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetConversationCountsResponse {
    pub counts: Vec<ConversationCount>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct DeleteConversationsRequest<'a> {
    #[serde(rename = "IDs")]
    pub ids: &'a [ConversationId],
    #[serde(rename = "LabelID")]
    pub label_id: &'a LabelId,
}

impl<'a> DeleteConversationsRequest<'a> {
    #[must_use]
    pub fn new(label_id: &'a LabelId, ids: &'a [ConversationId]) -> Self {
        Self { ids, label_id }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct DeleteConversationsResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<ConversationsResponseObject>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
pub struct ConversationsResponseObject {
    #[serde(rename = "ID")]
    pub id: ConversationId,
    #[serde(rename = "Response")]
    pub response: proton_api_core::APIErrorDesc,
}

impl<'c> http::RequestDesc for DeleteConversationsRequest<'c> {
    type Response = JsonResponse<DeleteConversationsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/delete").json(self)
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsReadRequest<'a> {
    #[serde(rename = "IDs")]
    pub ids: &'a [ConversationId],
}

impl<'a> MarkConversationsReadRequest<'a> {
    #[must_use]
    pub fn new(ids: &'a [ConversationId]) -> Self {
        Self { ids }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsReadResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<ConversationsResponseObject>,
}

impl<'c> http::RequestDesc for MarkConversationsReadRequest<'c> {
    type Response = JsonResponse<MarkConversationsReadResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/read").json(self)
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsUnreadRequest<'a> {
    #[serde(rename = "IDs")]
    pub ids: &'a [ConversationId],
}

impl<'a> MarkConversationsUnreadRequest<'a> {
    #[must_use]
    pub fn new(ids: &'a [ConversationId]) -> Self {
        Self { ids }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsUnreadResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<ConversationsResponseObject>,
}

impl<'c> http::RequestDesc for MarkConversationsUnreadRequest<'c> {
    type Response = JsonResponse<MarkConversationsUnreadResponse>;

    #[must_use]
    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/unread").json(self)
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct LabelConversationRequest<'a> {
    #[serde(rename = "LabelID")]
    pub label_id: &'a LabelId,
    #[serde(rename = "IDs")]
    pub ids: &'a [ConversationId],
    #[serde(serialize_with = "opt_bool_to_integer")]
    pub spam_action: Option<bool>,
    action: u32,
}

impl<'a> LabelConversationRequest<'a> {
    #[must_use]
    pub fn new(
        label_id: &'a LabelId,
        spam_action: Option<bool>,
        ids: &'a [ConversationId],
    ) -> Self {
        Self {
            label_id,
            ids,
            spam_action,
            action: 1,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct UnlabelConversationRequest<'a> {
    #[serde(rename = "LabelID")]
    pub label_id: &'a LabelId,
    #[serde(rename = "IDs")]
    pub ids: &'a [ConversationId],
}

impl<'a> UnlabelConversationRequest<'a> {
    #[must_use]
    pub fn new(label_id: &'a LabelId, ids: &'a [ConversationId]) -> Self {
        Self { label_id, ids }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct UndoToken {
    pub token: String,
    pub valid_until: u64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
pub struct LabelConversationsResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<ConversationsResponseObject>,
    pub undo_token: Option<UndoToken>,
}

impl<'a> http::RequestDesc for LabelConversationRequest<'a> {
    type Response = JsonResponse<LabelConversationsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/label").json(self)
    }
}

impl<'a> http::RequestDesc for UnlabelConversationRequest<'a> {
    type Response = JsonResponse<LabelConversationsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/unlabel").json(self)
    }
}
