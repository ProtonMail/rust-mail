//! This example demonstrates how to use an externally-managed auth session.

use anyhow::Result;
use muon::{App, Client, GET};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client.
    let app = App::new("windows-vpn@4.1.0")?;
    let client = Client::new(app, muon::env::EnvId::new_atlas())?;

    // Begin the auth flow.
    let auth = client.auth();

    // Provide an externally-managed auth UID.
    let client = auth.from_uid("abcdef", "abc123def456").await;

    // The client will **not** manage the tokens.
    // It assumes the tokens are managed externally.
    client.send(GET!("/tests/ping")).await?.ok()?;

    Ok(())
}
