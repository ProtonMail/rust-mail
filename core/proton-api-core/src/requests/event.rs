use crate::domain::IsEvent;
use crate::http;
use crate::http::RequestData;
use serde::Deserialize;
use std::marker::PhantomData;

#[doc(hidden)]
#[derive(Deserialize)]
pub struct LatestEventResponse {
    #[serde(rename = "EventID")]
    pub event_id: crate::domain::EventId,
}

pub struct GetLatestEventRequest;

impl http::RequestDesc for GetLatestEventRequest {
    type Response = http::JsonResponse<LatestEventResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Get, "core/v4/events/latest")
    }
}

pub struct GetEventRequest<'a, T: IsEvent> {
    event_id: &'a crate::domain::EventId,
    conversation_counts: bool,
    message_counts: bool,
    p: PhantomData<T>,
}

impl<'a, T: IsEvent> GetEventRequest<'a, T> {
    pub fn new(id: &'a crate::domain::EventId) -> Self {
        Self {
            event_id: id,
            conversation_counts: false,
            message_counts: false,
            p: PhantomData,
        }
    }

    pub fn with_counts(id: &'a crate::domain::EventId) -> Self {
        Self {
            event_id: id,
            conversation_counts: true,
            message_counts: true,
            p: PhantomData,
        }
    }
}

impl<'a, T: IsEvent> http::RequestDesc for GetEventRequest<'a, T> {
    type Response = http::JsonResponse<T>;

    fn build(&self) -> RequestData {
        let message_counts = if self.message_counts { "1" } else { "0" };
        let conversation_counts = if self.conversation_counts { "1" } else { "0" };
        RequestData::new(
            http::Method::Get,
            format!("core/v5/events/{}", self.event_id),
        )
        .query("MessageCounts", message_counts)
        .query("ConversationCounts", conversation_counts)
    }
}
