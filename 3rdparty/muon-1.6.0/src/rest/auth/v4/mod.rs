use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use tfa::TFA;

/// `/auth/v4/info`
pub mod info;

/// `/auth/v4/refresh`
pub mod refresh;

/// `/auth/v4/2fa`
pub mod tfa;

/// Fido2 related types
pub mod fido2;

/// `/auth/v4/sessions`
pub mod sessions;

/// `POST /auth/v4`
///
/// Authenticates a user using SRP.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Post {
    /// The SRP session ID (from `POST /auth/v4/info`).
    #[serde(rename = "SRPSession")]
    pub session: String,

    /// The username of the user to authenticate.
    pub username: String,

    /// The client's base64-encoded SRP ephemeral.
    pub client_ephemeral: String,

    /// The client's base64-encoded SRP proof.
    pub client_proof: String,

    /// The client's fingerprint, for anti-abuse.
    #[serde(rename = "Payload", skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Value>,
}

/// The response from a `POST /auth/v4` request.
///
/// Contains the auth tokens, 2FA information, and server proof.
/// The server proof should be used to verify the server's identity.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostRes {
    /// The server's SRP proof.
    pub server_proof: String,

    /// The password mode used by the account.
    pub password_mode: PasswordMode,

    /// The granted auth tokens.
    #[serde(flatten)]
    pub auth: Auth,

    /// The user's 2FA info.
    #[serde(default)]
    #[serde(rename = "2FA")]
    pub tfa: TFA,
}

/// The password mode used by the account.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum PasswordMode {
    /// The account has one password.
    One = 1,

    /// The account has two passwords.
    Two = 2,
}

/// `DELETE /auth/v4`
///
/// Logs out the user.
#[derive(Debug)]
pub struct Delete;

/// The primary auth type returned by the API.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Auth {
    /// The UID of the auth.
    #[serde(rename = "UID")]
    pub uid: String,

    /// The ID of the user that logged in, if any.
    #[serde(rename = "UserID")]
    pub user_id: Option<String>,

    /// The access token of the auth.
    pub access_token: String,

    /// The refresh token of the auth.
    pub refresh_token: String,

    /// The scopes this auth has.
    pub scopes: Vec<String>,
}
