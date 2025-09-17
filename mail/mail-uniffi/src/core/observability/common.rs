use proton_mail_common::MailUserContext;
use std::sync::Arc;
use tracing::error;

pub async fn should_record_telemetry(user_context: &Arc<MailUserContext>) -> bool {
    match user_context.user_settings().await {
        Ok(settings) => settings.telemetry,
        Err(err) => {
            error!("Failed to get user settings for telemetry check: {err:?}");
            false
        }
    }
}

#[cfg(test)]
pub mod test_helper {
    use proton_core_api::services::observability::{ObservabilityMetric, ObservabilityRecorder};
    use proton_core_api::services::proton::{PostMetricsRequestData, PostMetricsRequestElement};
    use serde_json::json;

    pub const TIMESTAMP: i64 = 1_741_021_308;
    pub const VALUE: u64 = 1;
    pub const STATUS: &str = "unknown";

    #[must_use]
    pub fn json(event_name: &str) -> String {
        format!(
            r#"{{"Name":"{event_name}","Version":1,"Timestamp":{TIMESTAMP},"Data":{{"Labels":{{"status":"{STATUS}"}},"Value":{VALUE}}}}}"#,
        )
    }

    pub fn serialize_metric<T: ObservabilityMetric>(test_metric: T) -> String {
        serde_json::to_string(
            &ObservabilityRecorder::into_metrics_element(test_metric, TIMESTAMP, VALUE).unwrap(),
        )
        .unwrap()
    }

    #[must_use]
    pub fn metric_request_element(event_name: &str) -> PostMetricsRequestElement {
        PostMetricsRequestElement {
            name: event_name.to_string(),
            version: 1,
            timestamp: TIMESTAMP,
            data: PostMetricsRequestData {
                labels: json!({ "status": STATUS}),
                value: VALUE,
            },
        }
    }
}
