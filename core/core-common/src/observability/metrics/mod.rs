use proton_core_api::services::observability::ApiServiceObservabilityResponse;
use proton_observability::metric;
use serde::{Deserialize, Serialize};

metric! {
    #[name = "core_signin_submit_mbp_total"]
    #[version = 1]
    pub struct SignInSubmitMailBoxPwTotal {
        pub status: MailboxPasswordMetricStatus
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[serde(untagged)]
pub enum MailboxPasswordMetricStatus {
    ApiService(ApiServiceObservabilityResponse),

    /// Indicates that the derivation of the key secret failed for the given key ID.
    /// This error occurs when either the matching salt cannot be found (e.g., `SaltError::KeyNotFound`
    /// or `SaltError::KeyHasNoSalt`) or the password derivation process fails.
    KeyDerivationFailed,

    /// Indicates that the decryption or unlocking of a key failed using the provided salted password.
    KeyUnlockFailed,
}

#[cfg(test)]
mod tests {
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };
    use proton_observability::into_metrics_element;

    use super::*;
    use serde_json::{self, json};

    #[test]
    fn test_signin_submit_mailbox_pw_total_serialization() {
        let metric = into_metrics_element(
            SignInSubmitMailBoxPwTotal {
                status: MailboxPasswordMetricStatus::ApiService(
                    ApiServiceObservabilityResponse::Success,
                ),
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_signin_submit_mbp_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"status":"success"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_signin_submit_mbp_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"status": "success"}),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
