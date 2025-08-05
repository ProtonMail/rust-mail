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

// Payments
#[derive(Debug, Copy, Clone, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityEvent {
    InAppPurchaseSubscribe,
    SendPaymentToken,
    CreateSubscription,
    GetPlans,
    GetSubscription,
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum PaymentObservabilityEventStatus {
    Success,
    Http4xx,
    Http5xx,
    SerializationError,
    Unknown,
}

#[uniffi_export]
pub fn send_payment_observability_event(
    event_type: PaymentObservabilityEvent,
    status: PaymentObservabilityEventStatus,
) {
    let recorder = ObservabilityRecorder::default();

    match event_type {
        PaymentObservabilityEvent::InAppPurchaseSubscribe => {
            recorder.record(InAppPurchaseSubscribeTotal::new(status));
        }
        PaymentObservabilityEvent::SendPaymentToken => {
            recorder.record(SendPaymentTokenTotal::new(status));
        }
        PaymentObservabilityEvent::CreateSubscription => {
            recorder.record(CreateSubscriptionTotal::new(status));
        }
        PaymentObservabilityEvent::GetPlans => {
            recorder.record(GetPlansTotal::new(status));
        }
        PaymentObservabilityEvent::GetSubscription => {
            recorder.record(GetSubscriptionTotal::new(status));
        }
    }
}

// Payments: IAP Subscribe
metric! {
    #[name = "payments_iap_subscribe_total"]
    #[version = 1]
    pub struct InAppPurchaseSubscribeTotal {
        pub status: PaymentObservabilityEventStatus
    }
}

// Payments: Token
metric! {
    #[name = "payments_iap_send_payment_token_total"]
    #[version = 1]
    pub struct SendPaymentTokenTotal {
        pub status: PaymentObservabilityEventStatus
    }
}

// Payments: Create Subscription
metric! {
    #[name = "payments_iap_create_subscription_total"]
    #[version = 1]
    pub struct CreateSubscriptionTotal {
        pub status: PaymentObservabilityEventStatus
    }
}

// Payments: Get Subscription
metric! {
    #[name = "payments_get_subscription_total"]
    #[version = 1]
    pub struct GetSubscriptionTotal {
        pub status: PaymentObservabilityEventStatus
    }
}

// Payments: Get Plans
metric! {
    #[name = "payments_get_plans_total"]
    #[version = 1]
    pub struct GetPlansTotal {
        pub status: PaymentObservabilityEventStatus
    }
}

// Payments: Tests
#[cfg(test)]
mod payments_tests {

    use super::*;
    use proton_core_api::services::{
        observability::ObservabilityRecorder,
        proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement},
    };
    use serde_json::{self, json};

    const EVENT_IAP_SUBSCRIBE: &str = "payments_iap_subscribe_total";
    const EVENT_SEND_PAYMENT_TOKEN: &str = "payments_iap_send_payment_token_total";
    const EVENT_CREATE_SUBSCRIPTION: &str = "payments_iap_create_subscription_total";
    const EVENT_GET_SUBSCRIPTION: &str = "payments_get_subscription_total";
    const EVENT_GET_PLANS: &str = "payments_get_plans_total";
    const TEST_DATA_TIMESTAMP: i64 = 1_741_021_308;
    const TEST_DATA_VALUE: u64 = 1;
    const TEST_DATA_STATUS: &str = "unknown";

    fn test_data_json(event_name: &str) -> String {
        format!(
            r#"{{"Name":"{event_name}","Version":1,"Timestamp":{TEST_DATA_TIMESTAMP},"Data":{{"Labels":{{"status":"{TEST_DATA_STATUS}"}},"Value":{TEST_DATA_VALUE}}}}}"#,
        )
    }

    fn test_serialized_metric<T: ObservabilityMetric>(test_metric: T) -> String {
        serde_json::to_string(
            &ObservabilityRecorder::into_metrics_element(
                test_metric,
                TEST_DATA_TIMESTAMP,
                TEST_DATA_VALUE,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn test_request_element(event_name: &str) -> PostMetricsRequestElement {
        PostMetricsRequestElement {
            name: event_name.to_string(),
            version: 1,
            timestamp: TEST_DATA_TIMESTAMP,
            data: PostMetricsRequestData {
                labels: json!({ "status": TEST_DATA_STATUS}),
                value: TEST_DATA_VALUE,
            },
        }
    }

    #[test]
    fn test_in_app_purchase_subscribe_total() {
        let serialized_metric = test_serialized_metric(InAppPurchaseSubscribeTotal {
            status: PaymentObservabilityEventStatus::Unknown,
        });

        assert_eq!(serialized_metric, test_data_json(EVENT_IAP_SUBSCRIBE));
        assert_eq!(
            test_request_element(EVENT_IAP_SUBSCRIBE),
            serde_json::de::from_str(&serialized_metric).unwrap(),
        );
    }

    #[test]
    fn test_send_payment_token_total() {
        let serialized_metric = test_serialized_metric(SendPaymentTokenTotal {
            status: PaymentObservabilityEventStatus::Unknown,
        });

        assert_eq!(serialized_metric, test_data_json(EVENT_SEND_PAYMENT_TOKEN));
        assert_eq!(
            test_request_element(EVENT_SEND_PAYMENT_TOKEN),
            serde_json::de::from_str(&serialized_metric).unwrap()
        );
    }

    #[test]
    fn test_create_subscription_total() {
        let serialized_metric = test_serialized_metric(CreateSubscriptionTotal {
            status: PaymentObservabilityEventStatus::Unknown,
        });

        assert_eq!(serialized_metric, test_data_json(EVENT_CREATE_SUBSCRIPTION));
        assert_eq!(
            test_request_element(EVENT_CREATE_SUBSCRIPTION),
            serde_json::de::from_str(&serialized_metric).unwrap()
        );
    }

    #[test]
    fn test_get_subscription_total() {
        let serialized_metric = test_serialized_metric(GetSubscriptionTotal {
            status: PaymentObservabilityEventStatus::Unknown,
        });

        assert_eq!(serialized_metric, test_data_json(EVENT_GET_SUBSCRIPTION));
        assert_eq!(
            test_request_element(EVENT_GET_SUBSCRIPTION),
            serde_json::de::from_str(&serialized_metric).unwrap()
        );
    }

    #[test]
    fn test_get_plans_total() {
        let serialized_metric = test_serialized_metric(GetPlansTotal {
            status: PaymentObservabilityEventStatus::Unknown,
        });

        assert_eq!(serialized_metric, test_data_json(EVENT_GET_PLANS));
        assert_eq!(
            test_request_element(EVENT_GET_PLANS),
            serde_json::de::from_str(&serialized_metric).unwrap()
        );
    }
}
