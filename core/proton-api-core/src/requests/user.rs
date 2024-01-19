use crate::domain::User;
use crate::http;
use crate::http::{JsonResponse, RequestData};
use proton_crypto_rs::salts::Salts;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserInfoResponse {
    pub user: User,
}

pub struct UserInfoRequest {}

impl http::RequestDesc for UserInfoRequest {
    type Output = UserInfoResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Get, "core/v4/users")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserSaltsResponse {
    pub key_salts: Salts,
}

pub struct GetUserSaltsRequest {}

impl http::RequestDesc for GetUserSaltsRequest {
    type Output = UserSaltsResponse;
    type Response = JsonResponse<Self::Output>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Get, "core/v4/keys/salts")
    }
}
