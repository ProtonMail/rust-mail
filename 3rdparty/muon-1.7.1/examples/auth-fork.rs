//! This example demonstrates how to fork a session to another client.

use anyhow::{bail, Result};
use muon::client::flow::{ForkFlowResult, LoginFlow, WithSelectorFlow};
use muon::{App, Client, GET};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client.
    let app = App::new("windows-vpn@4.1.0")?;
    let env = muon::env::EnvId::new_atlas();
    // Please check the auth-info-provider.rs example to see how to pass a
    // fingerprint to the muon client. The fingerprint is important in combating
    // fraud.
    let parent = Client::new(app, env)?;

    // Authenticate the client.
    let client = match parent.auth().login("visionary", "a").await {
        LoginFlow::Ok(client, _) => client,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected 2FA"),
        LoginFlow::Failed { reason, .. } => return Err(reason.into()),
    };

    // Fork the session to a windows-vpn client.
    let ForkFlowResult::Success(_, selector, _session_id) = client
        .fork("windows-vpn")
        .payload(b"hello world")
        .send()
        .await
    else {
        bail!("Fail to fork")
    };

    // Create a new windows-vpn client to take ownership of the fork.
    let app = App::new("windows-vpn@4.1.0")?;
    let env = muon::env::EnvId::new_atlas();
    let child = Client::new(app, env)?;

    // Authenticate the child client via the fork.
    let WithSelectorFlow::Ok(child, payload) =
        child.auth().from_fork().with_selector(selector).await
    else {
        bail!("couldn't log via fork selector")
    };

    // The payload is the data sent by the parent client.
    assert_eq!(payload.as_deref(), Some(b"hello world".as_ref()));

    // The child client is now authenticated.
    child.send(GET!("/core/v4/users")).await?.ok()?;

    Ok(())
}
