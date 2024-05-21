use crate::domain::{
    LabelId, Message, MessageCount, MessageId, MessageMetadata, MessageMetadataFilter,
};
use crate::{MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http;
use proton_api_core::http::{JsonResponse, Method, RequestData};
use proton_api_core::utils::{bool_from_integer, bool_to_integer};

pub struct GetMessageMetadataRequest {
    filter: MessageMetadataFilter,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageMetadataResponse {
    pub messages: Vec<MessageMetadata>,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub stale: bool,
    pub total: u64,
}

impl GetMessageMetadataRequest {
    #[must_use]
    pub fn new(mut filter: MessageMetadataFilter) -> Self {
        filter.page_size = filter.page_size.max(MAX_PAGE_ELEMENT_COUNT_U64);
        filter.limit = filter.limit.map(|v| v.max(MAX_LIMIT_VALUE_U64));
        Self { filter }
    }
}

impl http::RequestDesc for GetMessageMetadataRequest {
    type Response = JsonResponse<MessageMetadataResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Post, "mail/v4/messages")
            .header("X-HTTP-Method-Override", "GET")
            .json(&self.filter)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetMessageResponse {
    pub message: Message,
}

pub struct GetMessageRequest<'a> {
    id: &'a MessageId,
}
impl<'a> GetMessageRequest<'a> {
    #[must_use]
    pub fn new(id: &'a MessageId) -> Self {
        Self { id }
    }
}

impl<'a> http::RequestDesc for GetMessageRequest<'a> {
    type Response = JsonResponse<GetMessageResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("mail/v4/messages/{}", self.id))
    }
}

pub struct GetMessageCountsRequest {}
impl http::RequestDesc for GetMessageCountsRequest {
    type Response = JsonResponse<GetMessageCountsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "mail/v4/messages/count")
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetMessageCountsResponse {
    pub counts: Vec<MessageCount>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde")]
pub struct DeleteMessagesRequest<'a> {
    #[serde(rename = "IDs")]
    pub ids: &'a [MessageId],
    #[serde(rename = "CurrentLabelID")]
    pub label_id: Option<&'a LabelId>,
}

impl<'a> DeleteMessagesRequest<'a> {
    #[must_use]
    pub fn new(label_id: Option<&'a LabelId>, ids: &'a [MessageId]) -> Self {
        Self { ids, label_id }
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct DeleteMessagesResponse {
    #[serde(rename = "Responses")]
    pub responses: Vec<DeleteMessagesResponseObject>,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct DeleteMessagesResponseObject {
    #[serde(rename = "ID")]
    pub id: MessageId,
    #[serde(rename = "Response")]
    pub response: proton_api_core::APIErrorDesc,
}

impl<'c> http::RequestDesc for DeleteMessagesRequest<'c> {
    type Response = JsonResponse<DeleteMessagesResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Put, "mail/v4/conversations/delete").json(self)
    }
}
