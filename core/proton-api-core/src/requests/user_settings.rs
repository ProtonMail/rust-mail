use crate::domain::UserSettings;
use crate::http::{JsonResponse, Method, RequestData, RequestDesc};

pub struct UserSettingsRequest {}

impl RequestDesc for UserSettingsRequest {
    type Output = UserSettings;
    type Response = JsonResponse<UserSettings>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "core/v4/settings")
    }
}
