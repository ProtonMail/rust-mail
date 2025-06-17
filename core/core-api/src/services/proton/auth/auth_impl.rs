use crate::service::ApiServiceResult;
use crate::services::proton::Proton;
use crate::services::proton::auth::{AUTH_V4, ProtonAuth};
use crate::services::proton::prelude::*;
use muon::{GET, util::ProtonRequestExt};

impl ProtonAuth for Proton {
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse> {
        Ok(GET!("{AUTH_V4}/sessions/uuid")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
