use crate::service::ApiServiceResult;
use crate::services::proton::auth::{AUTH_V4, ProtonAuth};
use crate::services::proton::prelude::*;
use muon::common::Sender;
use muon::{GET, POST, util::ProtonRequestExt};
use muon::{ProtonRequest, ProtonResponse};

impl<This: ?Sized + Sender<ProtonRequest, ProtonResponse>> ProtonAuth for This {
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse> {
        Ok(GET!("{AUTH_V4}/sessions/uuid")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_auth_info(
        &self,
        request: PostAuthInfoRequest,
    ) -> ApiServiceResult<PostAuthInfoResponse> {
        Ok(POST!("{AUTH_V4}/info")
            .body_json(request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
