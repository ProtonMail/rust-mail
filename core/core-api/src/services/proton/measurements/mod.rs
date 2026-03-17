use requests::{PostMeasurementEventRequest, PostMeasurementEventsRequest};
use responses::PostMeasurementEventResponse;

use crate::service::ApiServiceResult;

mod measurements_impl;
mod request_data;
pub mod requests;
pub mod responses;

pub use self::request_data::*;

// TODO: This endpoint does not exist YET.
pub const MEASUREMENTS_V1: &str = "/growth/v1";

#[allow(async_fn_in_trait)]
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
