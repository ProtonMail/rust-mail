use muon::{POST, ProtonRequest, ProtonResponse, common::Sender, http::HttpReqExt};

use crate::service::ApiServiceResult;

use super::{
    MEASUREMENTS_V1, ProtonMeasurements,
    requests::{PostMeasurementEventRequest, PostMeasurementEventsRequest},
    responses::PostMeasurementEventResponse,
};

impl<This: ?Sized + Sender<ProtonRequest, ProtonResponse>> ProtonMeasurements for This {
    async fn post_event(
        &self,
        request: PostMeasurementEventRequest,
    ) -> ApiServiceResult<PostMeasurementEventResponse> {
        Ok(POST!("{MEASUREMENTS_V1}/event")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_events(
        &self,
        request: PostMeasurementEventsRequest,
    ) -> ApiServiceResult<PostMeasurementEventResponse> {
        Ok(POST!("{MEASUREMENTS_V1}/events")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
