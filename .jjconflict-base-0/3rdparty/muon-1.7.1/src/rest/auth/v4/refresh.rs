use crate::rest::auth::v4::Auth;
use serde::{Deserialize, Serialize};

/// `POST /auth/v4/refresh`
///
/// Refreshes the user's access token.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Post {
    /// The refresh token of the auth to refresh.
    pub refresh_token: String,

    /// The response type requested by the client.
    pub response_type: String,

    /// The grant type requested by the client.
    pub grant_type: String,

    /// The redirect URI of this request.
    #[serde(rename = "RedirectURI")]
    pub redirect_uri: String,
}

/// The response from a `POST /auth/v4/refresh` request.
///
/// Contains the newly-issued access token and refresh token.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostRes {
    /// The newly-issued auth tokens.
    #[serde(flatten)]
    pub auth: Auth,
}
