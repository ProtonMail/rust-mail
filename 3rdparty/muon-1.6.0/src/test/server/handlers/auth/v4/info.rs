use crate::rest::auth;
use crate::test::server::backend::Backend;
use crate::test::server::error::ServerRes;
use crate::util::ByteSliceExt;
use axum::extract::State;
use axum::Json;

/// Handle `POST /auth/v4/info`.
pub async fn post(
    State(this): State<Backend>,
    Json(body): Json<auth::v4::info::Post>,
) -> ServerRes<Json<auth::v4::info::PostRes>> {
    // Get the user's data.
    let user_id = this.get_user_id(&body.username).await?;
    let user = this.get_user(user_id).await?;

    // Begin the SRP auth.
    let (srp_id, challenge, modulus) = this.new_srp_session(user_id, user.verifier).await?;

    // Build the JSON response.
    let session = srp_id.to_string();
    let version = user.srp;
    let salt = user.salt.as_b64();
    let server_ephemeral = challenge.as_b64();

    // Build the JSON response.
    Ok(Json(auth::v4::info::PostRes {
        session,
        version,
        salt,
        modulus,
        server_ephemeral,
    }))
}
