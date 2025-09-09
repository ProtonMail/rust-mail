use anyhow::Result;
use muon::test::server::{Server, HTTPS};
use muon::tls::{RustlsTls, TokioTls};
use muon::GET;
use std::sync::Arc;

#[muon::test(scheme(HTTPS))]
async fn test_tls_rustls(s: Arc<Server>) -> Result<()> {
    let c = s.builder().tls(RustlsTls).build()?;

    c.send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}

#[muon::test(scheme(HTTPS))]
#[ignore = "self-signed certificate"]
async fn test_tls_tokio(s: Arc<Server>) -> Result<()> {
    let c = s.builder().tls(TokioTls).build()?;

    c.send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}
