#![allow(clippy::module_name_repetitions)] // to avoid issue with collisions in the requests namespace
use crate::auth::{AccessToken, RefreshToken, Scope};
use crate::domain::{LoginData, TFAStatus, Uid, UserId};
use crate::http;
use crate::http::{
    JsonResponse, RequestData, X_PM_HUMAN_VERIFICATION_TOKEN, X_PM_HUMAN_VERIFICATION_TOKEN_TYPE,
};
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;

#[doc(hidden)]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInfo<'a> {
    pub username: &'a str,
}

impl<'a> http::RequestDesc for AuthInfo<'a> {
    type Response = JsonResponse<AuthInfoResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/v4/info").json(self)
    }
}

#[doc(hidden)]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInfoResponse {
    pub version: u8,
    pub modulus: String,
    pub server_ephemeral: String,
    pub salt: String,
    #[serde(rename = "SRPSession")]
    pub srp_session: String,
}

#[doc(hidden)]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Auth<'a> {
    pub username: &'a str,
    pub client_ephemeral: &'a str,
    pub client_proof: &'a str,
    #[serde(rename = "SRPSession")]
    pub srp_session: &'a str,
    #[serde(skip)]
    pub human_verification: &'a Option<LoginData>,
}

impl<'a> http::RequestDesc for Auth<'a> {
    type Response = JsonResponse<AuthResponse>;

    fn build(&self) -> RequestData {
        let mut request = RequestData::new(http::Method::Post, "auth/v4").json(self);

        if let Some(hv) = &self.human_verification {
            // repeat submission with x-pm-human-verification-token and x-pm-human-verification-token-type
            request = request
                .header(X_PM_HUMAN_VERIFICATION_TOKEN, &hv.token)
                .header(X_PM_HUMAN_VERIFICATION_TOKEN_TYPE, hv.hv_type.as_str());
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
    pub scope: Scope,
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
    #[must_use]
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
    #[must_use]
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
pub struct AuthRefreshBody<'a> {
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
    pub scope: Scope,
}

pub struct AuthRefresh<'a> {
    uid: &'a Uid,
    token: &'a str,
}

impl<'a> AuthRefresh<'a> {
    #[must_use]
    pub fn new(uid: &'a Uid, token: &'a str) -> Self {
        Self { uid, token }
    }
}

impl<'a> http::RequestDesc for AuthRefresh<'a> {
    type Response = JsonResponse<AuthRefreshResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/v4/refresh").json(AuthRefreshBody {
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
    #[must_use]
    pub fn new(token: &'a str, force_web: bool) -> Self {
        Self { token, force_web }
    }
}

impl<'a> http::RequestDesc for CaptchaRequest<'a> {
    type Response = http::StringResponse;

    fn build(&self) -> RequestData {
        let mut data = RequestData::new(http::Method::Get, "core/v4/captcha");
        if self.force_web {
            data = data.query("ForceWebMessaging", &1);
        }

        data.query("Token", &self.token)
    }
}

/// Fork session request.
///
/// This request is used to fork a user's session, providing a new session for
/// the same user.
///
/// The general documentation for this can currently be found here:
///
///   - [Feature documentation](https://confluence.protontech.ch/display/CP/How+to+generate+a+session+fork+selector+for+testing+the+lite+account+application)
///
/// The required POST request is described as being:
///
///   - `POST /api/auth/sessions/forks`
///   - `{ ChildClientID: "web-account-lite", Independent: 0 }`
///
/// The headers should be taken care of by the general request-response process.
/// Therefore all this action needs to do is call the endpoint with the required
/// JSON body.
///
/// The relevant API documentation is here:
///
///   - [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/auth/#tag/Authentication-Sessions/operation/post_auth-%7B_version%7D-sessions-forks)
///
/// The fields in the JSON body are not currently documented.
///
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostUserForkSessionRequest {
    /// The child client ID, which is always `"web-account-lite"` at present. It
    /// seems like this is an identifier for the caller, but this is not clear.
    #[serde(rename = "ChildClientID")]
    pub child_client_id: String,

    /// It's not currently known what this does, and it's always set to `0`.
    pub independent: u8,
}

impl http::RequestDesc for PostUserForkSessionRequest {
    type Response = JsonResponse<UserForkSessionResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "auth/sessions/forks").json(self)
    }
}

/// Fork session response.
///
/// This is the "selector" that is returned when a session is forked.
///
/// The relevant API documentation is here:
///
///   - [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/auth/#tag/Authentication-Sessions/operation/post_auth-%7B_version%7D-sessions-forks)
///
/// The fields in the JSON response are not currently documented.
///
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserForkSessionResponse {
    /// The selector that is returned when a session is forked. It's not clear
    /// exactly what this is at present.
    pub selector: String,
}
