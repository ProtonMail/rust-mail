use crate::service::ApiServiceResult;
use crate::services::proton::DATA_V1;
use crate::services::proton::Proton;
use crate::services::proton::prelude::*;
use muon::POST;
use muon::common::ServiceType;
use muon::util::ProtonRequestExt;

impl ProtonData for Proton {
    async fn post_metrics(&self, metrics: Vec<PostMetricsRequestElement>) -> ApiServiceResult<()> {
        POST!("{DATA_V1}/metrics")
            .body_json(PostMetricsRequest { metrics })?
            .header(("Priority", "u=6"))
            .service_type(ServiceType::Background, true)
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }
}
