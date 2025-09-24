use crate::test::server::error::ServerRes;
use axum::body::Body;
use axum::response::Response;

/// Handle `POST /muon/bench`.
///
/// This is a simple echo route: it reads and returns the body as-is.
pub async fn post(body: Body) -> ServerRes<Response<Body>> {
    Ok(Response::new(body))
}
