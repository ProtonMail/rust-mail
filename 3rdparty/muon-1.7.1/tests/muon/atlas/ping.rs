use crate::atlas::new_client;
use anyhow::Result;
use muon::GET;

#[tokio::test]
async fn test_ping() -> Result<()> {
    let req = GET!("/tests/ping");

    new_client().send(req).await?.ok()?;

    Ok(())
}

#[tokio::test]
async fn test_ping_server() -> Result<()> {
    let servers = GET!("/tests/ping").servers([
        "https://verify-api.proton.black".parse()?,
        "https://verify.proton.black/api".parse()?,
    ]);

    new_client().send(servers).await?.ok()?;

    Ok(())
}
