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
    type Output = LatestEventResponse;
    type Response = http::JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Get, "core/v4/events/latest")
    }
}

pub struct GetEventRequest<'a, T: IsEvent> {
    event_id: &'a crate::domain::EventId,
    p: PhantomData<T>,
}

impl<'a, T: IsEvent> GetEventRequest<'a, T> {
    pub fn new(id: &'a crate::domain::EventId) -> Self {
        Self {
            event_id: id,
            p: PhantomData,
        }
    }
}

impl<'a, T: IsEvent> http::RequestDesc for GetEventRequest<'a, T> {
    type Output = T;
    type Response = http::JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(
            http::Method::Get,
            format!("core/v4/events/{}", self.event_id),
        )
    }
}
