use proton_observability::{PreLoginMetricRecorder, metric};
use serde::{Deserialize, Serialize};

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
    PreLoginMetricRecorder::default().record(AccountRecoveryScreenViewTotal::new(screen_id));
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
    PreLoginMetricRecorder::default().record(LoginScreenViewTotal::new(screen_id));
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
    PreLoginMetricRecorder::default().record(SignupScreenViewTotal::new(screen_id));
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
    PreLoginMetricRecorder::default().record(HumanVerificationScreenViewTotal::new(screen_id));
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
    PreLoginMetricRecorder::default().record(HumanVerificationResultTotal::new(status));
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
    PreLoginMetricRecorder::default().record(HumanVerificationViewLoadingResultTotal::new(status));
}
