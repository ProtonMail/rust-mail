use crate::auth::{AccessToken, AuthScope, RefreshToken};
use crate::domain::{HumanVerificationLoginData, TFAStatus, Uid, UserId};
use crate::http;
use crate::http::{RequestData, X_PM_HUMAN_VERIFICATION_TOKEN, X_PM_HUMAN_VERIFICATION_TOKEN_TYPE};
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;

#[doc(hidden)]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInfoRequest<'a> {
    pub username: &'a str,
}

impl<'a> http::RequestDesc for AuthInfoRequest<'a> {
    type Response = http::JsonResponse<AuthInfoResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/v4/info").json(self)
    }
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInfoResponse {
    pub version: i32,
    pub modulus: String,
    pub server_ephemeral: String,
    pub salt: String,
    #[serde(rename = "SRPSession")]
    pub srp_session: String,
}

#[doc(hidden)]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthRequest<'a> {
    pub username: &'a str,
    pub client_ephemeral: &'a str,
    pub client_proof: &'a str,
    #[serde(rename = "SRPSession")]
    pub srp_session: &'a str,
    #[serde(skip)]
    pub human_verification: &'a Option<HumanVerificationLoginData>,
}

impl<'a> http::RequestDesc for AuthRequest<'a> {
    type Response = http::JsonResponse<AuthResponse>;

    fn build(&self) -> RequestData {
        let mut request = RequestData::new(http::Method::Post, "auth/v4").json(self);

        if let Some(hv) = &self.human_verification {
            // repeat submission with x-pm-human-verification-token and x-pm-human-verification-token-type
            request = request
                .header(X_PM_HUMAN_VERIFICATION_TOKEN, &hv.token)
                .header(X_PM_HUMAN_VERIFICATION_TOKEN_TYPE, hv.hv_type.as_str())
        }

        request
    }
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AuthResponse {
    #[serde(rename = "UserID")]
    pub user_id: UserId,
    #[serde(rename = "UID")]
    pub uid: Uid,
    pub token_type: Option<String>,
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub server_proof: String,
    pub scope: AuthScope,
    #[serde(rename = "2FA")]
    pub tfa: TFAInfo,
    pub password_mode: PasswordMode,
}

#[doc(hidden)]
#[derive(Deserialize_repr, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum PasswordMode {
    One = 1,
    Two = 2,
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct TFAInfo {
    pub enabled: TFAStatus,
    #[serde(rename = "FIDO2")]
    pub fido2_info: FIDO2Info,
}

#[doc(hidden)]
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct FIDOKey {
    pub attestation_format: String,
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<i32>,
    pub name: String,
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct FIDO2Info {
    pub authentication_options: serde_json::Value,
    pub registered_keys: Option<serde_json::Value>,
}

#[doc(hidden)]
#[derive(Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct TFAAuth<'a> {
    pub two_factor_code: &'a str,
    #[serde(rename = "FIDO2")]
    pub fido2: FIDO2Auth<'a>,
}

#[doc(hidden)]
#[derive(Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct FIDO2Auth<'a> {
    pub authentication_options: serde_json::Value,
    pub client_data: &'a str,
    pub authentication_data: &'a str,
    pub signature: &'a str,
    #[serde(rename = "CredentialID")]
    pub credential_id: &'a [i32],
}

impl<'a> FIDO2Auth<'a> {
    pub fn empty() -> Self {
        FIDO2Auth {
            authentication_options: serde_json::Value::Null,
            client_data: "",
            authentication_data: "",
            signature: "",
            credential_id: &[],
        }
    }
}

pub struct TOTPRequest<'a> {
    code: &'a str,
}

impl<'a> TOTPRequest<'a> {
    pub fn new(code: &'a str) -> Self {
        Self { code }
    }
}

impl<'a> http::RequestDesc for TOTPRequest<'a> {
    type Response = http::NoResponse;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/v4/2fa").json(TFAAuth {
            two_factor_code: self.code,
            fido2: FIDO2Auth::empty(),
        })
    }
}

#[doc(hidden)]
#[derive(Serialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AuthRefresh<'a> {
    #[serde(rename = "UID")]
    pub uid: &'a str,
    pub refresh_token: &'a str,
    pub grant_type: &'a str,
    pub response_type: &'a str,
    #[serde(rename = "RedirectURI")]
    pub redirect_uri: &'a str,
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AuthRefreshResponse {
    #[serde(rename = "UID")]
    pub uid: Uid,
    pub token_type: Option<String>,
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub scope: AuthScope,
}

pub struct AuthRefreshRequest<'a> {
    uid: &'a Uid,
    token: &'a str,
}

impl<'a> AuthRefreshRequest<'a> {
    pub fn new(uid: &'a Uid, token: &'a str) -> Self {
        Self { uid, token }
    }
}

impl<'a> http::RequestDesc for AuthRefreshRequest<'a> {
    type Response = http::JsonResponse<AuthRefreshResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/v4/refresh").json(AuthRefresh {
            uid: &self.uid.0,
            refresh_token: self.token,
            grant_type: "refresh_token",
            response_type: "token",
            redirect_uri: "https://protonmail.ch/",
        })
    }
}

pub struct LogoutRequest {}

impl http::RequestDesc for LogoutRequest {
    type Response = http::NoResponse;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Delete, "auth/v4")
    }
}

pub struct CaptchaRequest<'a> {
    token: &'a str,
    force_web: bool,
}

impl<'a> CaptchaRequest<'a> {
    pub fn new(token: &'a str, force_web: bool) -> Self {
        Self { token, force_web }
    }
}

impl<'a> http::RequestDesc for CaptchaRequest<'a> {
    type Response = http::StringResponse;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(http::Method::Get, "core/v4/captcha");
        if self.force_web {
            data = data.query("ForceWebMessaging", 1)
        }

        data.query("Token", self.token)
    }
}
