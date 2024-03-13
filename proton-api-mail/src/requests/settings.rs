use crate::domain::MailSettings;
use proton_api_core::exports::serde::{self, Deserialize};
use proton_api_core::http::{JsonResponse, Method, RequestData, RequestDesc};

pub struct GetMailSettingsRequest {}

#[derive(Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MailSettingsResponse {
    pub mail_settings: MailSettings,
}

impl RequestDesc for GetMailSettingsRequest {
    type Output = MailSettingsResponse;
    type Response = JsonResponse<MailSettingsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, "mail/v4/settings")
    }
}
