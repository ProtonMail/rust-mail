use crate::rest::auth;
use crate::test::server::error::{cerr, ServerRes};
use axum::extract::State;
use axum::Json;
use crate::test::server::backend::{Auth, Backend};

/// Handle `POST /auth/v4/sessions`.
pub async fn post(
    State(this): State<Backend>
) -> ServerRes<Json<auth::v4::Auth>> {

    let uid = this.new_unauth_session().await?;
    let Auth { reftok, acctok, .. } = this.get_auth_session(uid).await?;
    let (acctok, scopes) = acctok.ok_or_else(|| cerr!(500, "missing auth session"))?;

    Ok(Json(auth::v4::Auth {
        uid: uid.to_string(),
        user_id: None,
        access_token: acctok.to_string(),
        refresh_token: reftok.to_string(),
        scopes: scopes.iter().map(ToString::to_string).collect(),
    }))
}