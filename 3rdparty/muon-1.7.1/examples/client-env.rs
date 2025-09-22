//! This example demonstrates the interplay between a client and an environment.

use anyhow::Result;
use muon::{App, Client, GET};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let c1 = new_client("prod")?;
    let c2 = new_client("atlas")?;
    let c3 = new_client("atlas:leopold")?;

    info!("{}", c1.send(GET!("/tests/ping")).await?);
    info!("{}", c2.send(GET!("/tests/ping")).await?);
    info!("{}", c3.send(GET!("/tests/ping")).await?);

    Ok(())
}

fn new_client(env: &str) -> Result<Client> {
    info!("creating client for environment {env}");

    let app = App::new("windows-vpn@4.0.1")?;
    let store = muon::env::EnvId::new_atlas();
    let client = Client::new(app, store)?;

    info!("created client with environment {:?}", client.env());

    Ok(client)
}
