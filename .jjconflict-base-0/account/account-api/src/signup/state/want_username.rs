use crate::shared::challenge::Behavior;
use crate::signup::SignupError;
use crate::signup::state::want_password::WantPassword;
use crate::signup::state::{StateData, StateResult, Username};
use crate::{AccountApi, ApiError, requests::ParseDomain};
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;
use proton_core_api::services::observability::ApiServiceObservabilityResponse;
use proton_observability::PreLoginMetricRecorder;
use proton_observability::metric;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Represents the state where the user needs to provide username.
#[derive(Debug, Display, Clone)]
#[display("WantUsername")]
pub struct WantUsername {
    client: Client,
    data: StateData,
    recorder: PreLoginMetricRecorder,
}

impl WantUsername {
    pub fn new(client: Client, data: StateData) -> Self {
        info!("Signup flow wants username");

        Self {
            client,
            data,
            recorder: PreLoginMetricRecorder::default(),
        }
    }

    /// Submits chosen username, confirming availability with `AccountApi::check_username_availability`.
    pub async fn submit_username(
        self,
        username: Username,
        behavior: Option<Behavior>,
    ) -> StateResult {
        info!("Submitting username");

        match username.clone() {
            Username::Internal { username, .. } => {
                self.client
                    .check_username_availability(username, ParseDomain::NoEmail, None)
                    .inspect_err(|err| {
                        self.recorder.record(UsernameAvailabilityStatus::error(
                            UsernameKind::Internal,
                            err,
                        ));
                    })
                    .inspect_ok(|_| {
                        self.recorder
                            .record(UsernameAvailabilityStatus::success(UsernameKind::Internal));
                    })
                    .map_err(|err| {
                        SignupError::UsernameUnavailable(err.err_info().and_then(|info| info.error))
                    })
                    .await?;
            }

            Username::External { email } => {
                self.client
                    .check_external_username_availability(email, None)
                    .inspect_err(|err| {
                        self.recorder.record(UsernameAvailabilityStatus::error(
                            UsernameKind::External,
                            err,
                        ));
                    })
                    .inspect_ok(|_| {
                        self.recorder
                            .record(UsernameAvailabilityStatus::success(UsernameKind::External));
                    })
                    .map_err(|err| {
                        SignupError::UsernameUnavailable(err.err_info().and_then(|info| info.error))
                    })
                    .await?;
            }
        }

        let mut data = self.data;
        data.challenge_info.username_behavior = behavior;

        Ok(WantPassword::new(self.client, username, data).into())
    }
}

#[derive(Display, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsernameKind {
    Internal,
    External,
}

metric! {
    #[name = "core_signup_username_availability_total"]
    #[version = 1]
    #[doc = "Records the outcomes of the `GET core/v4/users/available` and `GET core/v4/users/availableExternal` API calls on the origin device."]
    pub struct UsernameAvailabilityStatus {
        pub status: ApiServiceObservabilityResponse,
        pub kind: UsernameKind,
    }
}

impl UsernameAvailabilityStatus {
    fn success(kind: UsernameKind) -> Self {
        UsernameAvailabilityStatus {
            status: ApiServiceObservabilityResponse::Success,
            kind,
        }
    }
    fn error(kind: UsernameKind, error: &ApiError) -> Self {
        UsernameAvailabilityStatus {
            status: error.into(),
            kind,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };
    use proton_observability::into_metrics_element;
    use serde_json::{self, json};

    fn assert_serialization_deserialization(
        status: ApiServiceObservabilityResponse,
        expected_status: &str,
        kind: UsernameKind,
        expected_kind: &str,
    ) {
        let metric = into_metrics_element(
            UsernameAvailabilityStatus { status, kind },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        let expected_json = format!(
            r#"{{"Name":"core_signup_username_availability_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"kind":"{expected_kind}","status":"{expected_status}"}},"Value":1}}}}"#
        );
        let expected: serde_json::Value = serde_json::from_str(&expected_json).unwrap();
        assert_eq!(parsed, expected);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_signup_username_availability_total".into(),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "status": expected_status,
                        "kind": expected_kind
                    }),
                    value: 1,
                }
            },
            serde_json::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_username_availability_serialization_deserialization_for_all_variants() {
        let statuses = vec![
            (ApiServiceObservabilityResponse::Success, "success"),
            (ApiServiceObservabilityResponse::Http4xx, "http4xx"),
            (ApiServiceObservabilityResponse::Http5xx, "http5xx"),
            (
                ApiServiceObservabilityResponse::NetworkError,
                "network_error",
            ),
            (
                ApiServiceObservabilityResponse::SerializationError,
                "serialization_error",
            ),
            (ApiServiceObservabilityResponse::Unknown, "unknown"),
        ];

        for (status, expected_status) in statuses {
            assert_serialization_deserialization(
                status,
                expected_status,
                UsernameKind::Internal,
                "internal",
            );
        }
    }
}
