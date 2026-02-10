mod auth_impl;
mod responses;

pub use self::responses::*;
use crate::service::ApiServiceResult;
use muon::rest::auth::v4::{fido2, tfa::TFA};
use serde::{Deserialize, Serialize};

/// The Proton Auth API base path (v4).
pub const AUTH_V4: &str = "/auth/v4";

/// `POST /auth/v4/info`
///
/// Initializes the SRP authentication process for the given user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthInfoRequest {
    /// The username of the user to authenticate.
    pub username: String,
}

/// The response from a `POST /auth/v4/info` request.
///
/// Contains the SRP parameters needed to authenticate the user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthInfoResponse {
    /// The SRP session ID.
    #[serde(rename = "SRPSession")]
    pub session: String,

    /// The SRP version used by the server.
    pub version: u8,

    /// The user's salt.
    pub salt: String,

    /// The server's SRP modulus.
    pub modulus: String,

    /// The server's SRP ephemeral.
    pub server_ephemeral: String,

    /// The user's 2FA info (only if already logged in).
    #[serde(default)]
    #[serde(rename = "2FA")]
    pub tfa: Option<TFA>,
}

impl PostAuthInfoResponse {
    /// Returns FIDO2 keys and auth options.
    #[must_use]
    pub fn fido_details(&self) -> Option<fido2::Response> {
        self.tfa.as_ref()?.fido_details()
    }
}

impl Clone for PostAuthInfoResponse {
    fn clone(&self) -> Self {
        serde_json::to_value(self)
            .and_then(serde_json::from_value)
            .unwrap()
    }
}

#[allow(async_fn_in_trait)]
pub trait ProtonAuth {
    /// GET the user's session UUID.
    async fn get_sessions_uuid(&self) -> ApiServiceResult<GetSessionsUuidResponse>;

    /// POST auth info to initialize SRP authentication.
    ///
    /// This endpoint initializes the SRP authentication process for the given user
    /// and returns the SRP parameters needed for authentication.
    ///
    async fn post_auth_info(
        &self,
        request: PostAuthInfoRequest,
    ) -> ApiServiceResult<PostAuthInfoResponse>;
}
