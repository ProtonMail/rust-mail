use serde::{Deserialize, Serialize};

/// `POST /auth/v4/info`
///
/// Initializes the SRP authentication process for the given user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Post {
    /// The username of the user to authenticate.
    pub username: String,
}

/// The response from a `POST /auth/v4/info` request.
///
/// Contains the SRP parameters needed to authenticate the user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostRes {
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
}
