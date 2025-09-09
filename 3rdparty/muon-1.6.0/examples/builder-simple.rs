//! This example demonstrates the muon builder.

use anyhow::Result;
use muon::rt::{AsyncDialer, AsyncResolver};
use muon::{App, Client};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client builder.
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    let builder = Client::builder(app, store);

    // We can set custom resolvers, dialers, enable DNS/DNS-over-HTTPS services.
    let builder = builder.resolver(AsyncResolver).dialer(AsyncDialer);

    // Finally, create the client.
    let _ = builder.build();

    Ok(())
}
