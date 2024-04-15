use proton_mail_common::proton_api_mail::proton_api_core::login::{Error, Flow};
use secrecy::SecretString;

#[derive(Debug)]
pub enum LoginEvent {
    LoginRequest {
        user: String,
        password: SecretString,
    },
    TwoFARequest(String),
    LoginFailed(Error),
    LoginSuccess(Flow),
    LoginSuccess2FA(Flow),
    LoginNeed2FA(Flow),
    Login2FAFailed((Flow, Error)),
    Logout,
}
