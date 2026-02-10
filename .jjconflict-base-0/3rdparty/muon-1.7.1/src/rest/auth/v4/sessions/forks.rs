use crate::rest::auth::v4::Auth;
use crate::rest::Bool;
use serde::{Deserialize, Serialize};

/// `GET /auth/v4/sessions/forks`
///
/// Randomly select a human-friendly identifier to perform a fork.
#[derive(Debug)]
pub struct Get;

/// The response from a `GET /auth/v4/sessions/forks` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRes {
    /// Random 20-char selector.
    pub selector: String,

    /// Random 8-char user code.
    #[serde(rename = "UserCode")]
    pub code: String,
}

/// `POST /auth/v4/sessions/forks`
///
/// Performs a session fork.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Post {
    /// The client ID of the child session.
    #[serde(rename = "ChildClientID")]
    pub child: String,

    /// Whether the child session should be independent.
    #[serde(default)]
    pub independent: Bool,

    /// The fork payload.
    #[serde(default)]
    pub payload: Option<String>,

    /// The user code to use.
    #[serde(rename = "UserCode")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// The response from a `POST /auth/v4/sessions/forks` request.
///
/// Contains the selector.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostRes {
    /// The selector returned by the API when a fork is requested.
    pub selector: String,
}

/// `GET /auth/v4/sessions/forks/{id}`
///
/// Get a forked session.
#[derive(Debug)]
pub struct GetId {
    /// The ID of the forked session to get.
    pub id: String,
}

/// The response from a `GET /auth/v4/sessions/forks/{id}` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetIdRes {
    /// The base64-encoded, encrypted payload.
    pub payload: Option<String>,

    /// The auth tokens for the forked session.
    #[serde(flatten)]
    pub auth: Auth,
}
