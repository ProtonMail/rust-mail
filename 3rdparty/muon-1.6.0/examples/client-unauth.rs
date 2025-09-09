//! This example demonstrates how a un-authenticated client sends a request

use anyhow::Result;
use muon::{App, Client, GET};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let app = App::new("windows-vpn@4.0.1")?;
    let env = muon::env::EnvId::new_prod();
    let client = Client::new(app, env)?;

    info!("created client with environment {:?}", client.env());

    info!("{}", client.send(GET!("/tests/ping")).await?);

    Ok(())
}
