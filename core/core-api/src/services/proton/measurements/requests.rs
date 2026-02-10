use std::collections::BTreeMap;

use serde::Serialize;

use super::request_data::{MeasurementEventType, MeasurementValue};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PostMeasurementEventRequest {
    event_type: MeasurementEventType,
    /// UTC
    event_timestamp_ms: i64,
    /// Android ASID
    asid: String,
    /// The app's package or bundle identifier
    app_package_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_start_ms: Option<i64>,

    // We are explicitly not hardcoding every field,
    // Instead mobile dev gets a flexible endpoint.
    // In case new field is introduced - no Rust change is needed.
    // In case of new event-type, only one flat enum has to change.
    #[serde(flatten)]
    fields: BTreeMap<String, MeasurementValue>,
}

pub type PostMeasurementEventsRequest = Vec<PostMeasurementEventRequest>;
