use crate::domain::{
    Conversation, ConversationCount, ConversationFilter, ConversationId, LabelId, MessageMetadata,
};
use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http;
use proton_api_core::http::{JsonResponse, Method, RequestData};

pub struct GetConversationsRequest {
    filter: ConversationFilter,
}

impl GetConversationsRequest {
    pub fn new(filter: ConversationFilter) -> Self {
        Self { filter }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetConversationsResponse {
    pub conversations: Vec<Conversation>,
    pub stale: ProtonBoolean,
    pub total: u64,
}

impl http::RequestDesc for GetConversationsRequest {
    type Output = GetConversationsResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(Method::Get, "mail/v4/conversations")
            .query("Page", self.filter.page)
            .query("PageSize", self.filter.page_size);

        if let Some(ids) = &self.filter.ids {
            data = data.query_array("ID", ids);
        }

        if let Some(subject) = &self.filter.subject {
            data = data.query("Subject", subject);
        }

        if let Some(addr_id) = &self.filter.address_id {
            data = data.query("AddressID", addr_id);
        }

        if let Some(label_ids) = &self.filter.label_id {
            data = data.query_array("LabelID", label_ids);
        }

        if let Some(external_id) = &self.filter.external_id {
            data = data.query("ExternalID", external_id);
        }

        if let Some(end_id) = &self.filter.end_id {
            data = data.query("EndID", end_id);
        }

        if let Some(sort) = &self.filter.sort {
            data = data.query("Sort", sort);
        }

        data.query("Desc", self.filter.desc)
    }
}

pub struct GetConversationRequest<'a> {
    id: &'a ConversationId,
}

impl<'a> GetConversationRequest<'a> {
    pub fn new(id: &'a ConversationId) -> Self {
        Self { id }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetConversationResponse {
    pub conversation: Conversation,
    pub messages: Vec<MessageMetadata>,
}

impl<'a> http::RequestDesc for GetConversationRequest<'a> {
    type Output = GetConversationResponse;
    type Response = JsonResponse<Self::Output>;
    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("mail/v4/conversations/{}", self.id))
    }
}

pub struct GetConversationCountsRequest {}
impl http::RequestDesc for GetConversationCountsRequest {
    type Output = GetConversationCountsResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "mail/v4/conversations/count")
    }
}

#[derive(Debug, Deserialize)]
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
    pub fn new(label_id: &'a LabelId, ids: &'a [ConversationId]) -> Self {
        Self { ids, label_id }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct DeleteConversationsResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<DeleteConversationsResponseObject>,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct DeleteConversationsResponseObject {
    #[serde(rename = "ID")]
    pub id: ConversationId,
    #[serde(rename = "Response")]
    pub response: proton_api_core::APIErrorDesc,
}

impl<'c> http::RequestDesc for DeleteConversationsRequest<'c> {
    type Output = DeleteConversationsResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/delete").json(self)
    }
}
