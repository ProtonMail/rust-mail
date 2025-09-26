use proton_observability::{PreLoginMetricRecorder, metric};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "snake_case")]
pub enum ChangePasswordScreenId {
    ChangePassword,
    ChangeMailboxPassword,
    #[serde(rename = "change_password_2fa")]
    ChangePassword2fa,
}

metric! {
    #[name = "core_change_password_screen_view_total"]
    #[version = 1]
    pub struct ChangePasswordScreenViewTotal {
        pub screen_id: ChangePasswordScreenId,
    }
}

#[uniffi_export]
pub fn record_change_password_screen_view(screen_id: ChangePasswordScreenId) {
    PreLoginMetricRecorder::default().record(ChangePasswordScreenViewTotal::new(screen_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };
    use proton_observability::into_metrics_element;
    use serde_json::{self, json};

    #[test]
    fn test_change_password_screen_view_total_serialization() {
        let metric = into_metrics_element(
            ChangePasswordScreenViewTotal {
                screen_id: ChangePasswordScreenId::ChangePassword2fa,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_change_password_screen_view_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"change_password_2fa"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_change_password_screen_view_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "screen_id": "change_password_2fa"
                    }),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
