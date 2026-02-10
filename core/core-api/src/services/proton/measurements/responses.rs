use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct PostMeasurementEventResponse {
    #[serde(default)]
    pub session_start_ms: Option<i64>,
    // Note: This field exists in response specs but we decided not to use it.
    // To prevent accidental use I commented it but I'm leaving this comment
    // so its clear this is intentional and not a mistake.
    // #[serde(default)]
    // pub deeplink: Option<string>
}
