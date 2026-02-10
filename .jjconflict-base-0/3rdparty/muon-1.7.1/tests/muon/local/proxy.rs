use anyhow::Result;
use muon::common::ConstProxy;
use muon::test::proxy;
use muon::test::server::{Server, HTTPS};
use muon::util::ProtonRequestExt;
use muon::GET;
use std::sync::Arc;

#[muon::test]
#[cfg_attr(ci, ignore = "local proxy not supported in CI")]
async fn test_ping_proxy_http(s: Arc<Server>) -> Result<()> {
    let proxy = ConstProxy::new(proxy::url()?.try_into()?);
    let client = s.builder().proxy(proxy).build()?;

    GET!("/tests/ping").send_with(&client).await?.ok()?;

    Ok(())
}

#[muon::test(scheme(HTTPS))]
#[cfg_attr(ci, ignore = "local proxy not supported in CI")]
async fn test_ping_proxy_https(s: Arc<Server>) -> Result<()> {
    let proxy = ConstProxy::new(proxy::url()?.try_into()?);
    let client = s.builder().proxy(proxy).build()?;

    GET!("/tests/ping").send_with(&client).await?.ok()?;

    Ok(())
}
