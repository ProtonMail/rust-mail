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

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum HumanVerificationViewLoadingStatus {
    Http2xx,
    Http4xx,
    Http400,
    Http404,
    Http422,
    Http5xx,
    ConnectionError,
    SslError,
}

metric! {
    #[name = "core_human_verification_view_loading_result_total"]
    #[version = 1]
    pub struct HumanVerificationViewLoadingResultTotal {
        pub status: HumanVerificationViewLoadingStatus,
    }
}

#[uniffi_export]
pub fn record_human_verification_view_loading_result(status: HumanVerificationViewLoadingStatus) {
    ObservabilityRecorder::default().record(HumanVerificationViewLoadingResultTotal::new(status));
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

    fn assert_human_verification_result_serialization_deserialization(
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
            assert_human_verification_result_serialization_deserialization(status, expected_status);
        }
    }

    fn assert_human_verification_view_loading_result_serialization_deserialization(
        status: HumanVerificationViewLoadingStatus,
        expected_status: &str,
    ) {
        let metric = ObservabilityRecorder::into_metrics_element(
            HumanVerificationViewLoadingResultTotal { status },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();

        let expected_json = format!(
            r#"{{"Name":"core_human_verification_view_loading_result_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"status":"{expected_status}"}},"Value":1}}}}"#
        );

        assert_eq!(serialized, expected_json);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_human_verification_view_loading_result_total".into(),
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
    fn test_human_verification_view_loading_result() {
        let statuses = vec![
            (HumanVerificationViewLoadingStatus::Http2xx, "http2xx"),
            (HumanVerificationViewLoadingStatus::Http4xx, "http4xx"),
            (HumanVerificationViewLoadingStatus::Http400, "http400"),
            (HumanVerificationViewLoadingStatus::Http404, "http404"),
            (HumanVerificationViewLoadingStatus::Http422, "http422"),
            (HumanVerificationViewLoadingStatus::Http5xx, "http5xx"),
            (
                HumanVerificationViewLoadingStatus::ConnectionError,
                "connectionError",
            ),
            (HumanVerificationViewLoadingStatus::SslError, "sslError"),
        ];

        for (status, expected_status) in statuses {
            assert_human_verification_view_loading_result_serialization_deserialization(
                status,
                expected_status,
            );
        }
    }
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum FidoLaunchResultStatus {
    Success,
    Failure,
}

metric! {
    #[name = "core_signin_secondFactor_fido_launchResult_total"]
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
    #[name = "core_signin_secondFactor_fido_signResult_total"]
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

// Payments
// Payments: Models
#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityMetric {
    IapSubscribe(PaymentObservabilityResponse),
    SendPaymentToken(PaymentObservabilityResponse),
    CreateSubscription(PaymentObservabilityResponse),
    GetSubscription(PaymentObservabilityResponse),
    GetPlans(PaymentObservabilityResponse),
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityResponse {
    Success,
    Http4xx,
    Http5xx,
    SerializationError,
    Unknown,
}

// Payments: Metrics
metric! {
    #[name = "payments_iap_subscribe_total"]
    #[version = 1]
    pub struct IapSubscribeMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_iap_send_payment_token_total"]
    #[version = 1]
    pub struct SendPaymentTokenMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_iap_create_subscription_total"]
    #[version = 1]
    pub struct CreateSubscriptionMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_get_subscription_total"]
    #[version = 1]
    pub struct GetSubscriptionMetric { pub status: PaymentObservabilityResponse }
}
metric! {
    #[name = "payments_get_plans_total"]
    #[version = 1]
    pub struct GetPlansMetric { pub status: PaymentObservabilityResponse }
}

// Payments: Metric Recording
impl PaymentObservabilityMetric {
    pub fn record(self, recorder: &ObservabilityRecorder) {
        match self {
            Self::IapSubscribe(response) => {
                recorder.record(IapSubscribeMetric::new(response));
            }
            Self::SendPaymentToken(response) => {
                recorder.record(SendPaymentTokenMetric::new(response));
            }
            Self::CreateSubscription(response) => {
                recorder.record(CreateSubscriptionMetric::new(response));
            }
            Self::GetSubscription(response) => {
                recorder.record(GetSubscriptionMetric::new(response));
            }
            Self::GetPlans(response) => {
                recorder.record(GetPlansMetric::new(response));
            }
        }
    }
}

// Payments: Function Exposure
#[uniffi_export]
pub fn send_payment_observability_metric(metric: PaymentObservabilityMetric) {
    let recorder = ObservabilityRecorder::default();
    metric.record(&recorder);
}

// Payments: Tests
#[cfg(test)]
mod payments_tests {
    use super::*;
    use serde_json::{self};

    mod test_data {
        pub const TIMESTAMP: i64 = 1_741_021_308;
        pub const VALUE: u64 = 1;
        pub const STATUS: &str = "unknown";

        pub fn json(event_name: &str) -> String {
            format!(
                r#"{{"Name":"{event_name}","Version":1,"Timestamp":{TIMESTAMP},"Data":{{"Labels":{{"status":"{STATUS}"}},"Value":{VALUE}}}}}"#,
            )
        }
    }

    mod test_helper {
        use crate::core::observability::payments_tests::test_data;
        use proton_core_api::services::observability::{
            ObservabilityMetric, ObservabilityRecorder,
        };
        use proton_core_api::services::proton::{
            PostMetricsRequestData, PostMetricsRequestElement,
        };
        use serde_json::json;

        pub fn serialize_metric<T: ObservabilityMetric>(test_metric: T) -> String {
            serde_json::to_string(
                &ObservabilityRecorder::into_metrics_element(
                    test_metric,
                    test_data::TIMESTAMP,
                    test_data::VALUE,
                )
                .unwrap(),
            )
            .unwrap()
        }
        pub fn metric_request_element(event_name: &str) -> PostMetricsRequestElement {
            PostMetricsRequestElement {
                name: event_name.to_string(),
                version: 1,
                timestamp: test_data::TIMESTAMP,
                data: PostMetricsRequestData {
                    labels: json!({ "status": test_data::STATUS}),
                    value: test_data::VALUE,
                },
            }
        }
    }

    #[test]
    fn test_iap_subscribe_metric() {
        const EVENT: &str = "payments_iap_subscribe_total";
        let serialized = test_helper::serialize_metric(IapSubscribeMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_data::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_send_payment_token_metric() {
        const EVENT: &str = "payments_iap_send_payment_token_total";
        let serialized = test_helper::serialize_metric(SendPaymentTokenMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_data::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_create_subscription_metric() {
        const EVENT: &str = "payments_iap_create_subscription_total";
        let serialized = test_helper::serialize_metric(CreateSubscriptionMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_data::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_get_subscription_metric() {
        const EVENT: &str = "payments_get_subscription_total";
        let serialized = test_helper::serialize_metric(GetSubscriptionMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_data::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_get_plans_metric() {
        const EVENT: &str = "payments_get_plans_total";
        let serialized = test_helper::serialize_metric(GetPlansMetric {
            status: PaymentObservabilityResponse::Unknown,
        });

        assert_eq!(serialized, test_data::json(EVENT));
        assert_eq!(
            test_helper::metric_request_element(EVENT),
            serde_json::de::from_str(&serialized).unwrap()
        );
    }
}
