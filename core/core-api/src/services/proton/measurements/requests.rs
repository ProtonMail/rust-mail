use std::collections::HashMap;

use serde::Serialize;

use super::request_data::{MeasurementEventType, MeasurementValue};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PostMeasurementEventRequest {
    pub event_type: MeasurementEventType,
    /// UTC
    pub event_timestamp_ms: u128,
    /// Android ASID
    pub asid: String,
    /// The app's package or bundle identifier
    pub app_package_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_start_ms: Option<u128>,

    // We are explicitly not hardcoding every field,
    // Instead mobile dev gets a flexible endpoint.
    // In case new field is introduced - no Rust change is needed.
    // In case of new event-type, only one flat enum has to change.
    #[serde(flatten)]
    pub fields: HashMap<String, Option<MeasurementValue>>,
}

pub type PostMeasurementEventsRequest = Vec<PostMeasurementEventRequest>;
