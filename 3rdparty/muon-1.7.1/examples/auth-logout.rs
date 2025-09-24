//! This example demonstrates the basic login flow for a muon client.

use anyhow::{bail, Result};
use muon::client::flow::LoginFlow;
use muon::{App, Client, GET};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client.
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    // Please check the auth-info-provider.rs example to see how to pass a
    // fingerprint to the muon client. The fingerprint is important in combating
    // fraud.
    let parent = Client::new(app, store)?;

    // Authenticate the client.
    let client = match parent.auth().login("visionary", "a").await {
        LoginFlow::Ok(client, _) => client,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected 2FA"),
        LoginFlow::Failed { reason, .. } => return Err(reason.into()),
    };

    println!("{}", client.send(GET!("/core/v4/users")).await?.ok()?);

    // Now logout.
    client.logout().await;
    // you can still ping
    client.send(GET!("/tests/ping")).await?.ok()?;
    // but you can't query authenticated routes
    assert!(client.send(GET!("/core/v4/users")).await?.ok().is_err());

    Ok(())
}
