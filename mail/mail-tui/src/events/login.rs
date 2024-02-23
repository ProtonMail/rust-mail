use crate::state::{DataLoadError, UserState};
use proton_api_mail::proton_api_core::http::HttpRequestError;
use proton_api_mail::proton_api_core::{LoginError, TotpSession};

pub enum LoginEvents {
    LoginFailed(LoginError),
    LoginSuccess(Result<UserState, DataLoadError>),
    LoginNeed2FA(TotpSession),
    Login2FAFailed(HttpRequestError),
}
