use anyhow::Result;
use muon::test::server::{Server, HTTP, HTTPS};
use muon::GET;
use std::sync::Arc;

#[muon::test(scheme(HTTP))]
async fn test_ping_http(s: Arc<Server>) -> Result<()> {
    s.client().send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}

#[muon::test(scheme(HTTPS))]
async fn test_ping_https(s: Arc<Server>) -> Result<()> {
    s.client().send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}
