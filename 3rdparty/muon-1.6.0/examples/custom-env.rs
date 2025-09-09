//! This example demonstrates creating a custom env with a local IP address.

use anyhow::Result;
use muon::app::AppVersion;
use muon::common::Server;
use muon::env::Env;
use muon::util::ProtonRequestExt;
use muon::{App, Client, GET};

/// A custom environment.
struct MyCustomEnv;

/// Implement [`MyCustomEnv`] to specify the servers for the custom environment.
impl Env for MyCustomEnv {
    fn servers(&self, _: &AppVersion) -> Vec<Server> {
        vec!["http://localhost:1234".parse().unwrap()]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client targeting the custom environment.
    let app = App::new("windows-vpn@4.1.0")?.with_user_agent("Mozilla/5.0");
    let store = muon::env::EnvId::new_custom(MyCustomEnv);
    let client = Client::new(app, store)?;

    // Requests will now be sent to the custom environment.
    GET!("/").send_with(&client).await?.ok()?;

    Ok(())
}
