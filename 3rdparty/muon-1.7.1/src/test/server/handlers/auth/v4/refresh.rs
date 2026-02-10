use crate::rest::auth;
use crate::test::server::backend::Backend;
use crate::test::server::error::{cerr, ServerRes};
use axum::extract::State;
use axum::http::header::HeaderMap;
use axum::Json;
use std::borrow::ToOwned;

/// Handle `POST /auth/v4/refresh`.
pub async fn post(
    State(this): State<Backend>,
    headers: HeaderMap,
    Json(body): Json<auth::v4::refresh::Post>,
) -> ServerRes<Json<auth::v4::refresh::PostRes>> {
    // Get the UID from the `x-pm-uid` header.
    let uid = headers
        .get("x-pm-uid")
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
        .ok_or_else(|| cerr!(400, "missing x-pm-uid header"))?;

    // Get the refresh token from the request.
    let tok = body.refresh_token;

    // Grant new auth tokens.
    let uid = this.refresh_auth_session(&uid, &tok).await?;
    let auth = this.get_auth_session(uid).await?;
    let (acctok, scopes) = (auth.acctok).ok_or_else(|| cerr!(500, "missing auth session"))?;

    // Build the JSON response.
    Ok(Json(auth::v4::refresh::PostRes {
        auth: auth::v4::Auth {
            uid: uid.to_string(),
            user_id: Some(auth.user_id.to_string()),
            access_token: acctok.to_string(),
            refresh_token: auth.reftok.to_string(),
            scopes: scopes.iter().map(ToString::to_string).collect(),
        },
    }))
}
