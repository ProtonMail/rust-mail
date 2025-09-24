use crate::rest::auth;
use crate::rest::auth::v4::tfa::TFA;
use crate::test::server::backend::{Auth, Backend};
use crate::test::server::error::{cerr, ServerRes};
use crate::util::ByteSliceExt;
use axum::extract::State;
use axum::Json;

/// `/auth/v4/info`
pub mod info;

/// `/auth/v4/refresh`
pub mod refresh;

/// `/auth/v4/2fa`
pub mod tfa;

/// `/auth/v4/sessions`
pub mod sessions;

/// Handle `POST /auth/v4`.
pub async fn post(
    State(this): State<Backend>,
    Json(body): Json<auth::v4::Post>,
) -> ServerRes<Json<auth::v4::PostRes>> {
    let user_id = this.get_user_id(&body.username).await?;

    // Decode the client ephemeral.
    let ephemeral: Vec<u8> = body
        .client_ephemeral
        .b64_into()
        .map_err(|_| cerr!(400, "invalid client ephemeral"))?;

    // Decode the client proof.
    let proof: Vec<u8> = body
        .client_proof
        .b64_into()
        .map_err(|_| cerr!(400, "invalid client proof"))?;

    // Verify the SRP proof.
    let proof = this
        .verify_srp_proof(user_id, &body.session, &ephemeral, &proof)
        .await?;

    // Grant the auth tokens.
    let uid = this.new_auth_session(user_id).await?;
    let Auth { reftok, acctok, .. } = this.get_auth_session(uid).await?;
    let (acctok, scopes) = acctok.ok_or_else(|| cerr!(500, "missing auth session"))?;

    // Build the JSON response.
    Ok(Json(auth::v4::PostRes {
        server_proof: proof.as_b64(),
        password_mode: auth::v4::PasswordMode::One,
        auth: auth::v4::Auth {
            uid: uid.to_string(),
            user_id: Some(user_id.to_string()),
            access_token: acctok.to_string(),
            refresh_token: reftok.to_string(),
            scopes: scopes.iter().map(ToString::to_string).collect(),
        },
        tfa: TFA::default(),
    }))
}
