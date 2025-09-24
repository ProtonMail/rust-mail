//! This example demonstrates how to work with muon responses.

use anyhow::{bail, Result};
use muon::client::flow::LoginFlow;
use muon::json::Value;
use muon::{App, Client, Status, GET};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client.
    let app = App::new("windows-vpn@4.1.0")?;
    let store = muon::env::EnvId::new_atlas();
    // Please check the auth-info-provider.rs example to see how to pass a
    // fingerprint to the muon client. The fingerprint is important in combating
    // fraud.
    let client = Client::new(app, store)?;

    // Login with the client.
    let client = match client.auth().login("visionary", "a").await {
        LoginFlow::Ok(client, _) => client,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected 2FA"),
        LoginFlow::Failed { reason, .. } => return Err(reason.into()),
    };

    // Get a response.
    let res = client.send(GET!("/core/v4/users")).await?;

    // First useful thing is if it was 2xx.
    let res = res.ok()?;

    // Inspect the status code in various ways.
    match res.status() {
        s if s.is_success() => info!("success!"),
        s if s.is_redirection() => warn!("redirect!"),
        s if s == 404 => error!("not found!"),
        s if s == Status::UNAUTHORIZED => error!("unauthorized!"),
        s => warn!("unexpected status {s}!"),
    };

    // Get the headers.
    let headers = res.headers();

    // Get one header.
    if let Some(duration) = headers.get("retry-after") {
        info!("retry-after: {}", duration.to_str()?);
    }

    // Iterate over all headers.
    for (name, value) in headers {
        info!("{name}: {}", value.to_str()?);
    }

    // Get the body as a reference to raw bytes.
    let _ = res.body();

    // Deserialize the body as JSON.
    let _: Value = res.body_json()?;

    // Consume the response, returning the body as raw bytes.
    let _ = res.into_body();

    Ok(())
}
