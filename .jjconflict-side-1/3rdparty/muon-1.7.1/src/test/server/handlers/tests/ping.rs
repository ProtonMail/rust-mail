use crate::test::server::error::ServerRes;

/// Handle `GET /tests/ping`.
pub async fn get() -> ServerRes<()> {
    Ok(())
}
