use crate::service::ApiServiceResult;
use crate::services::proton::auth::{ProtonAuth, AUTH_V4};
use crate::services::proton::prelude::*;
use crate::services::proton::Proton;
use muon::{util::ProtonRequestExt, GET};

impl ProtonAuth for Proton {
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse> {
        Ok(GET!("{AUTH_V4}/sessions/uuid")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
