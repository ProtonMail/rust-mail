use requests::{PostMeasurementEventRequest, PostMeasurementEventsRequest};
use responses::PostMeasurementEventResponse;

use crate::service::ApiServiceResult;

mod measurements_impl;
mod request_data;
mod requests;
mod responses;

// TODO: This endpoint does not exist YET.
#[allow(dead_code)]
pub const MEASUREMENTS_V1: &str = "/api/v1/measurement/";

#[allow(async_fn_in_trait, dead_code)]
pub trait ProtonMeasurements {
    /// NOTE: This endpoint is made solely for Android
    async fn post_event(
        &self,
        request: PostMeasurementEventRequest,
    ) -> ApiServiceResult<PostMeasurementEventResponse>;

    /// NOTE: This endpoint is made solely for Android
    async fn post_events(
        &self,
        request: PostMeasurementEventsRequest,
    ) -> ApiServiceResult<PostMeasurementEventResponse>;
}
