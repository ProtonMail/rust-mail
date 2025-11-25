use proton_core_api::services::observability::ApiServiceObservabilityResponse;

use crate::metric;

metric! {
    #[name = "core_signin_auth_total"]
    #[version = 1]
    #[doc = "Tracks the success or failure of the POST request to `/auth/v4/info` and `/auth/v4`."]
    #[doc = "This metric indicates whether the authentication session initialization/login request completed successfully."]
    pub struct AuthV4RequestMetric {
        pub status: ApiServiceObservabilityResponse
    }
}

metric! {
    #[name = "core_signin_submit_totp_total"]
    #[version = 1]
    pub struct SignInSubmitTotpTotal {
        pub status: ApiServiceObservabilityResponse
    }
}

metric! {
    #[name = "core_signin_submit_fido_total"]
    #[version = 1]
    pub struct SignInSubmitFidoTotal {
        pub status: ApiServiceObservabilityResponse
    }
}

#[cfg(test)]
mod tests {
    use crate::into_metrics_element;
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };

    use super::*;
    use serde_json::{self, json};

    #[test]
    fn test_signin_auth_serialization() {
        let metric = into_metrics_element(
            AuthV4RequestMetric {
                status: ApiServiceObservabilityResponse::Success,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_signin_auth_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"status":"success"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_signin_auth_total"),
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

    #[test]
    fn test_signin_submit_totp_total_serialization() {
        let metric = into_metrics_element(
            SignInSubmitTotpTotal {
                status: ApiServiceObservabilityResponse::Success,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_signin_submit_totp_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"status":"success"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_signin_submit_totp_total"),
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

    #[test]
    fn test_signin_submit_fido_total_serialization() {
        let metric = into_metrics_element(
            SignInSubmitFidoTotal {
                status: ApiServiceObservabilityResponse::Success,
            },
            1_741_021_308,
            2,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_signin_submit_fido_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"status":"success"},"Value":2}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_signin_submit_fido_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"status": "success"}),
                    value: 2,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
