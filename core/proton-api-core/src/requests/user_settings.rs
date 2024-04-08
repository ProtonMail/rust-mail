use crate::domain::UserSettings;
use crate::http::{JsonResponse, Method, RequestData, RequestDesc};
use serde::{Deserialize, Serialize};

pub struct UserSettingsRequest {}

#[derive(Serialize, Deserialize)]
pub struct UserSettingsResponse {
    #[serde(rename = "UserSettings")]
    pub user_settings: UserSettings,
}
impl RequestDesc for UserSettingsRequest {
    type Response = JsonResponse<UserSettingsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "core/v4/settings")
    }
}
