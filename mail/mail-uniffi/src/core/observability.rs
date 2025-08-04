use proton_core_api::{
    metric,
    services::observability::{ObservabilityMetric, ObservabilityRecorder},
};
use serde::{Deserialize, Serialize};
use uniffi_runtime::async_runtime;

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum AccountRecoveryScreenId {
    GracePeriodInfo,
    CancelResetPassword,
    PasswordChangeInfo,
    RecoveryCancelledInfo,
    RecoveryExpiredInfo,
}

metric! {
    #[name = "core_accountRecovery_screenView_total"]
    #[version = 1]
    pub struct AccountRecoveryScreenViewTotal {
        pub screen_id: AccountRecoveryScreenId,
    }
}

#[uniffi_export]
pub fn record_account_recovery_screen_view(screen_id: AccountRecoveryScreenId) {
    ObservabilityRecorder::default().record(AccountRecoveryScreenViewTotal::new(screen_id));
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum LoginScreenId {
    ChooseInternalAddress,
    SignInWithSso,
    SsoIdentityProvider,
    SecondFactor,
    SignInWithUsernamePassword,
    MailboxPassword,
}

metric! {
    #[name = "core_login_screenView_total"]
    #[version = 1]
    pub struct LoginScreenViewTotal {
        pub screen_id: LoginScreenId
    }
}

#[uniffi_export]
pub fn record_login_screen_view(screen_id: LoginScreenId) {
    ObservabilityRecorder::default().record(LoginScreenViewTotal::new(screen_id));
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum SignupScreenId {
    ChooseExternalEmail,
    ChooseInternalEmail,
    ChooseUsername,
    CreatePassword,
    SetRecoveryMethod,
    Congratulations,
}

metric! {
    #[name = "core_signup_screenView_total"]
    #[version = 1]
    pub struct SignupScreenViewTotal {
        pub screen_id: SignupScreenId
    }
}

#[uniffi_export]
pub fn record_signup_screen_view(screen_id: SignupScreenId) {
    ObservabilityRecorder::default().record(SignupScreenViewTotal::new(screen_id));
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum HumanVerificationScreenId {
    V3,
}

metric! {
    #[name = "core_human_verification_screen_view_total"]
    #[version = 1]
    pub struct HumanVerificationScreenViewTotal {
        pub screen_id: HumanVerificationScreenId
    }
}

#[uniffi_export]
pub fn record_human_verification_screen_view(screen_id: HumanVerificationScreenId) {
    ObservabilityRecorder::default().record(HumanVerificationScreenViewTotal::new(screen_id));
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum HumanVerificationStatus {
    Succeeded,
    Failed,
    Cancelled,
}

metric! {
    #[name = "core_human_verification_result_total"]
    #[version = 1]
    pub struct HumanVerificationResultTotal {
        pub status: HumanVerificationStatus
    }
}

#[uniffi_export]
pub fn record_human_verification_result(status: HumanVerificationStatus) {
    ObservabilityRecorder::default().record(HumanVerificationResultTotal::new(status));
}

#[cfg(test)]
mod tests {

    use super::*;
    use proton_core_api::services::proton::PostMetricsRequestData;
    use proton_core_api::services::proton::PostMetricsRequestElement;
    use serde_json::{self, json};

    #[test]
    fn test_account_recovery_screen() {
        let metric = ObservabilityRecorder::into_metrics_element(
            AccountRecoveryScreenViewTotal {
                screen_id: AccountRecoveryScreenId::GracePeriodInfo,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_accountRecovery_screenView_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"gracePeriodInfo"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_accountRecovery_screenView_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"screen_id": "gracePeriodInfo"}),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_login_screen() {
        let metric = ObservabilityRecorder::into_metrics_element(
            LoginScreenViewTotal {
                screen_id: LoginScreenId::ChooseInternalAddress,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_login_screenView_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"chooseInternalAddress"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_login_screenView_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"screen_id": "chooseInternalAddress"}),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_signup_screen() {
        let metric = ObservabilityRecorder::into_metrics_element(
            SignupScreenViewTotal {
                screen_id: SignupScreenId::ChooseExternalEmail,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_signup_screenView_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"chooseExternalEmail"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_signup_screenView_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"screen_id": "chooseExternalEmail"}),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_human_verification_screen() {
        let metric = ObservabilityRecorder::into_metrics_element(
            HumanVerificationScreenViewTotal {
                screen_id: HumanVerificationScreenId::V3,
            },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();
        assert_eq!(
            serialized,
            r#"{"Name":"core_human_verification_screen_view_total","Version":1,"Timestamp":1741021308,"Data":{"Labels":{"screen_id":"v3"},"Value":1}}"#
        );
        assert_eq!(
            PostMetricsRequestElement {
                name: String::from("core_human_verification_screen_view_total"),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"screen_id": "v3"}),
                    value: 1,
                }
            },
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    fn assert_serialization_deserialization(
        status: HumanVerificationStatus,
        expected_status: &str,
    ) {
        let metric = ObservabilityRecorder::into_metrics_element(
            HumanVerificationResultTotal { status },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();

        let expected_json = format!(
            r#"{{"Name":"core_human_verification_result_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"status":"{expected_status}"}},"Value":1}}}}"#
        );

        assert_eq!(serialized, expected_json);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_human_verification_result_total".into(),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({"status": expected_status}),
                    value: 1,
                }
            },
            serde_json::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_human_verification_result() {
        let statuses = vec![
            (HumanVerificationStatus::Succeeded, "succeeded"),
            (HumanVerificationStatus::Failed, "failed"),
            (HumanVerificationStatus::Cancelled, "cancelled"),
        ];

        for (status, expected_status) in statuses {
            assert_serialization_deserialization(status, expected_status);
        }
    }
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum FidoLaunchResultStatus {
    Success,
    Failure,
}

metric! {
    #[name = "signin_secondFactor_fido_launchResult_total"]
    #[version = 1]
    pub struct SecondFactorFidoLaunchResultTotal {
        pub screen_id: FidoLaunchResultStatus
    }
}

#[uniffi_export]
pub fn record_fido_launch_result(result: FidoLaunchResultStatus) {
    async_runtime().block_on(async move {
        ObservabilityRecorder::default().record(SecondFactorFidoLaunchResultTotal::new(result));
    });
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum FidoSignResultStatus {
    Empty,
    Success,
    UserCancelled,
    FailureNotSupported,
    FailureInvalidState,
    FailureSecurity,
    FailureNetwork,
    FailureAbort,
    FailureTimeout,
    FailureEncoding,
    FailureConstraint,
    FailureData,
    FailureNotAllowed,
    FailureAttestationNotPrivate,
    FailureUnknown,
    FailureNoResponse,
    Unknown,
}

metric! {
    #[name = "signin_secondFactor_fido_signResult_total"]
    #[version = 1]
    pub struct SecondFactorFidoSignResultTotal {
        pub screen_id: FidoSignResultStatus
    }
}

#[uniffi_export]
pub fn record_fido_sign_result(result: FidoSignResultStatus) {
    async_runtime().block_on(async move {
        ObservabilityRecorder::default().record(SecondFactorFidoSignResultTotal::new(result));
    });
}
