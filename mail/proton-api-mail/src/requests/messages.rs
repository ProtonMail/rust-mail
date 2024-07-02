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
        filter.page_size = filter.page_size.min(MAX_PAGE_ELEMENT_COUNT_U64);
        filter.limit = filter.limit.map(|v| v.min(MAX_LIMIT_VALUE_U64));
        Self { filter }
    }
}

impl http::RequestDesc for GetMessageMetadataRequest {
    type Response = JsonResponse<MessageMetadataResponse>;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(Method::Get, "mail/v4/messages")
            .query("Page", &self.filter.page)
            .query("PageSize", &self.filter.page_size);

        if let Some(limit) = &self.filter.limit {
            data = data.query("Limit", limit);
        }

        if let Some(label_id) = &self.filter.label_id {
            data = data.query_array("LabelID", label_id);
        }

        if let Some(sort) = &self.filter.sort {
            data = data.query("Sort", sort);
        }

        if let Some(desc) = self.filter.desc {
            data = data.query("Desc", if desc { &1 } else { &0 });
        }

        if let Some(begin) = &self.filter.begin {
            data = data.query("Begin", begin);
        }

        if let Some(end) = &self.filter.end {
            data = data.query("End", end);
        }

        if let Some(begin_id) = &self.filter.begin_id {
            data = data.query("BeginID", begin_id);
        }

        if let Some(end_id) = &self.filter.end_id {
            data = data.query("EndID", end_id);
        }

        if let Some(keyword) = &self.filter.keyword {
            data = data.query("Keyword", keyword);
        }

        if let Some(recipients) = &self.filter.recipients {
            data = data.query_array("Recipients", recipients);
        }

        if let Some(to) = &self.filter.to {
            data = data.query_array("To", to);
        }

        if let Some(cc) = &self.filter.cc {
            data = data.query_array("CC", cc);
        }

        if let Some(bcc) = &self.filter.bcc {
            data = data.query_array("BCC", bcc);
        }

        if let Some(from) = &self.filter.from {
            data = data.query("From", from);
        }

        if let Some(subject) = &self.filter.subject {
            data = data.query("Subject", subject);
        }

        if let Some(attachments) = self.filter.attachments {
            data = data.query("Attachments", if attachments { &1 } else { &0 });
        }

        if let Some(unread) = self.filter.unread {
            data = data.query("Unread", if unread { &1 } else { &0 });
        }

        if let Some(id) = &self.filter.conversation_id {
            data = data.query("ConversationID", id);
        }

        if let Some(addr_id) = &self.filter.address_id {
            data = data.query("AddressID", addr_id);
        }

        if let Some(ids) = &self.filter.ids {
            data = data.query_array("ID", ids);
        }

        if let Some(external_id) = &self.filter.external_id {
            data = data.query("ExternalID", external_id);
        }

        if let Some(wildcard) = self.filter.auto_wildcard {
            data = data.query("AutoWildcard", if wildcard { &1 } else { &0 });
        }

        data
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
