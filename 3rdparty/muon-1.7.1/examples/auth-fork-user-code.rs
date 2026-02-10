//! This example demonstrates how to fork a session to another client.

use anyhow::{bail, Result};
use muon::client::flow::{LoginFlow, WithCodeFlow};
use muon::util::DurationExt;
use muon::{App, Client, GET};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Create a parent client.
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    // Please check the auth-info-provider.rs example to see how to pass a
    // fingerprint to the muon client. The fingerprint is important in combating
    // fraud.
    let parent = Client::new(app, store)?;

    // Authenticate the parent client.
    let client = match parent.auth().login("visionary", "a").await {
        LoginFlow::Ok(client, _) => client,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected 2FA"),
        LoginFlow::Failed { reason, .. } => return Err(reason.into()),
    };

    // Create a child client.
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    let child = Client::new(app, store)?;

    // Child requests a fork from the parent via a code.
    let WithCodeFlow::Poll(flow) = child.auth().from_fork().with_code().await else {
        bail!("unexpected success or failure")
    };

    let code = flow.code().to_owned();

    // Code is displayed to the user.
    info!(%code, "user code to complete fork");

    // No fork available yet; parent hasn't entered the code.
    let mut flow = match flow.poll().await {
        WithCodeFlow::Poll(flow) => flow,
        _ => bail!("unexpected success or failure"),
    };

    // Parent enters the code.
    let _ = client
        .fork("windows-vpn")
        .payload(b"hello world")
        .code(&code)
        .send()
        .await;

    // Polling should now succeed.
    let (child, payload) = loop {
        info!("polling");

        flow = match flow.poll().await {
            WithCodeFlow::Ok(child, payload) => break (child, payload),
            WithCodeFlow::Poll(flow) => flow,
            WithCodeFlow::Failed { .. } => bail!("failed to log via fork"),
        };

        tokio::time::sleep(1.s()).await;
    };

    // The payload is the data sent by the parent client.
    assert_eq!(payload.as_deref(), Some(b"hello world".as_ref()));

    // The child client is now authenticated.
    println!("{}", child.send(GET!("/core/v4/users")).await?.ok()?);

    Ok(())
}
