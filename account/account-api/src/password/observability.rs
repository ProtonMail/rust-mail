use super::PasswordError;
use crate::password::{
    FlowAuthError,
    state::{State, StateData},
};
use proton_core_api::services::observability::ApiServiceObservabilityResponse;
use proton_core_common::{
    metric,
    observability::{ObservabilityMetric, PreLoginMetricRecorder},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasswordMode {
    Disabled,
    Enabled,
}

impl From<bool> for PasswordMode {
    fn from(value: bool) -> Self {
        if value {
            PasswordMode::Enabled
        } else {
            PasswordMode::Disabled
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ObservableData {
    mbp: PasswordMode,
    totp: PasswordMode,
    fido: PasswordMode,
}

impl ObservableState for StateData {
    fn observable_data(&self) -> ObservableData {
        ObservableData {
            mbp: self.mbp_mode.has_mbp().into(),
            totp: self.tfa_mode.has_totp().into(),
            fido: self.tfa_mode.has_fido().into(),
        }
    }
}

pub trait ObservableState {
    fn observable_data(&self) -> ObservableData;
}

pub trait ObservableResult {
    fn observe(self, recorder: &PreLoginMetricRecorder, data: ObservableData) -> Self;
}

impl ObservableResult for Result<State, PasswordError> {
    fn observe(self, recorder: &PreLoginMetricRecorder, data: ObservableData) -> Self {
        let status = match &self {
            Ok(state) => match state {
                State::Complete => ApiServiceObservabilityResponse::Success,
                State::WantTfa(_) | State::WantChange | State::Invalid => {
                    // Only send observability event on completion or when first error occurs.
                    return self;
                }
            },
            Err(error) => error.into(),
        };

        recorder.record(ChangePasswordUpdateLoginPasswordTotal::new(status, data));

        self
    }
}

metric! {
    #[name = "core_change_password_update_password_total"]
    #[version = 1]
    pub struct ChangePasswordUpdateLoginPasswordTotal {
        pub status: ApiServiceObservabilityResponse,
        #[serde(flatten)]
        pub observable_data: ObservableData,
    }
}

impl From<&PasswordError> for ApiServiceObservabilityResponse {
    fn from(error: &PasswordError) -> Self {
        match error {
            PasswordError::Api(api_error) => api_error.into(),
            PasswordError::ApiService(api_service_error) => api_service_error.into(),
            PasswordError::FlowAuth(flow_auth_error) => match flow_auth_error {
                FlowAuthError::Other(api_error) => api_error.into(),
                FlowAuthError::PasswordWrong(_) => Self::Http4xx,
            },
            PasswordError::ServerProof
            | PasswordError::MissingPrimaryKey
            | PasswordError::KeySecretSaltFetch(_)
            | PasswordError::KeySecretDerivation(_)
            | PasswordError::KeySecretDecryption
            | PasswordError::KeyEncoding(_)
            | PasswordError::Crypto(_)
            | PasswordError::Store(_)
            | PasswordError::InvalidState => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };
    use proton_core_common::observability::into_metrics_element;
    use serde_json::{self, json};

    #[test]
    fn test_change_password_update_login_password_total_serialization() {
        let metric = into_metrics_element(
            ChangePasswordUpdateLoginPasswordTotal {
                status: ApiServiceObservabilityResponse::Success,
                observable_data: ObservableData {
                    mbp: PasswordMode::Enabled,
                    totp: PasswordMode::Disabled,
                    fido: PasswordMode::Disabled,
                },
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_change_password_update_password_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"fido":"disabled","mbp":"enabled","status":"success","totp":"disabled"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_change_password_update_password_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "status": "success",
                        "mbp": "enabled",
                        "totp": "disabled",
                        "fido": "disabled"
                    }),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_change_password_update_login_password_total_error_serialization() {
        let metric = into_metrics_element(
            ChangePasswordUpdateLoginPasswordTotal {
                status: ApiServiceObservabilityResponse::NetworkError,
                observable_data: ObservableData {
                    mbp: PasswordMode::Disabled,
                    totp: PasswordMode::Enabled,
                    fido: PasswordMode::Disabled,
                },
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_change_password_update_password_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"fido":"disabled","mbp":"disabled","status":"network_error","totp":"enabled"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_change_password_update_password_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "status": "network_error",
                        "mbp": "disabled",
                        "totp": "enabled",
                        "fido": "disabled"
                    }),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
