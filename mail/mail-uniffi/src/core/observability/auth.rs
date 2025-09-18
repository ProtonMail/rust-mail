use proton_core_common::{
    metric,
    observability::{ObservabilityMetric, ObservabilityRecorder},
};
use serde::{Deserialize, Serialize};
use uniffi_runtime::async_runtime;

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
        ObservabilityRecorder::default()
            .record(SecondFactorFidoLaunchResultTotal::new(result), true);
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
        ObservabilityRecorder::default().record(SecondFactorFidoSignResultTotal::new(result), true);
    });
}
