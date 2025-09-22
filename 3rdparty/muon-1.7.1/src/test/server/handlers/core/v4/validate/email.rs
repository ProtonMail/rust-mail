use crate::test::server::backend::Backend;
use crate::test::server::error::ServerRes;
use axum::extract::{Request, State};

/// Handle `POST /core/v4/validate/email`
pub async fn post(State(_): State<Backend>, _: Request) -> ServerRes<()> {
    Ok(())
}
