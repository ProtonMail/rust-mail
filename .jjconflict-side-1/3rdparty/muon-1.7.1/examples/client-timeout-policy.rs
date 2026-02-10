//! This example demonstrates how to add timeouts to a single request.

use anyhow::Result;
use muon::util::{DurationExt, ProtonRequestExt};
use muon::{App, Client, GET};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client builder.
    let app = App::new("windows-vpn@4.1.0")?.with_user_agent("Mozilla/5.0");
    let store = muon::env::EnvId::new_atlas();
    let client = Client::builder(app, store).build()?;
    // Set the timeout policy for a single request
    GET!("/tests/ping")
        .allowed_time(2.s())
        .send_with(&client)
        .await?
        .ok()?;

    Ok(())
}
