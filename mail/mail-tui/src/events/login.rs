use proton_mail_common::proton_api_mail::proton_api_core::login::{LoginFlow, LoginFlowError};
use secrecy::SecretString;

#[derive(Debug)]
pub enum LoginEvent {
    LoginRequest {
        user: String,
        password: SecretString,
    },
    TwoFARequest(String),
    LoginFailed(LoginFlowError),
    LoginSuccess(LoginFlow),
    LoginSuccess2FA(LoginFlow),
    LoginNeed2FA(LoginFlow),
    Login2FAFailed((LoginFlow, LoginFlowError)),
    Logout,
}
