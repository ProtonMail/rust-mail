use crate::rest::core::v4::keys::Key;
use crate::rest::core::v4::users::User;
use crate::rest::{core, Bool};
use crate::test::server::backend::{Backend, UserId};
use crate::test::server::error::ServerRes;
use axum::extract::{Request, State};
use axum::Json;
use itertools::Itertools;

/// Handle `GET /core/v4/users`.
pub async fn get(
    State(this): State<Backend>,
    req: Request,
) -> ServerRes<Json<core::v4::users::GetRes>> {
    // Get the user ID and scopes from the request.
    let &user_id = req.extensions().get::<UserId>().unwrap();

    // Get the user's data.
    let user = this.get_user(user_id).await?;
    let keys = this.get_keys(&user.keys).await?;

    // Build the keys list.
    let keys = keys
        .into_iter()
        .map(|(key_id, key)| Key {
            id: key_id.to_string(),
            private_key: key.key,
            token: None,
            signature: None,
            primary: Bool::from(key_id == user.keys[0]),
            active: Bool::from(key.active),
        })
        .collect_vec();

    // Build the JSON response.
    Ok(Json(core::v4::users::GetRes {
        user: User {
            id: user_id.to_string(),
            name: user.name,
            email: user.email,
            keys,
        },
    }))
}
