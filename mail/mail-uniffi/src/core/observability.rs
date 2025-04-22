use proton_api_core::{
    metric,
    services::observability::{ObservabilityMetric, ObservabilityRecorder},
};
use serde::{Deserialize, Serialize};

use crate::async_runtime;

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
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
    async_runtime().block_on(async move {
        ObservabilityRecorder::default().record(AccountRecoveryScreenViewTotal::new(screen_id));
    });
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
pub enum LoginScreenId {
    ChooseInternalAddress,
    SignInWithSso,
    SsoIdentityProvider,
    SecondFactor,
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
    async_runtime().block_on(async move {
        ObservabilityRecorder::default().record(LoginScreenViewTotal::new(screen_id));
    });
}

#[derive(Debug, Serialize, Deserialize, uniffi::Enum)]
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
    async_runtime().block_on(async move {
        ObservabilityRecorder::default().record(SignupScreenViewTotal::new(screen_id));
    });
}
