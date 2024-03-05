use crate::domain::{Message, MessageCount, MessageId, MessageMetadata, MessageMetadataFilter};
use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde::{self, Deserialize};
use proton_api_core::http;
use proton_api_core::http::{JsonResponse, Method, RequestData};

pub struct GetMessageMetadataRequest {
    filter: MessageMetadataFilter,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageMetadataResponse {
    pub messages: Vec<MessageMetadata>,
    pub stale: ProtonBoolean,
    pub total: u64,
}

impl GetMessageMetadataRequest {
    pub fn new(filter: MessageMetadataFilter) -> Self {
        Self { filter }
    }
}

impl http::RequestDesc for GetMessageMetadataRequest {
    type Output = MessageMetadataResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "mail/v4/messages")
            .header("X-HTTP-Method-Override", "GET")
            .json(&self.filter)
    }
}

#[derive(Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetMessageResponse {
    pub message: Message,
}

pub struct GetMessageRequest<'a> {
    id: &'a MessageId,
}
impl<'a> GetMessageRequest<'a> {
    pub fn new(id: &'a MessageId) -> Self {
        Self { id }
    }
}

impl<'a> http::RequestDesc for GetMessageRequest<'a> {
    type Output = GetMessageResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("mail/v4/messages/{}", self.id))
    }
}

pub struct GetMessageCountsRequest {}
impl http::RequestDesc for GetMessageCountsRequest {
    type Output = GetMessageCountsResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "mail/v4/messages/count")
    }
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetMessageCountsResponse {
    pub counts: Vec<MessageCount>,
}
