//! This example demonstrates how to add timeouts to a single request.

use anyhow::Result;
use muon::common::RetryPolicy;
use muon::util::{DurationExt, ProtonRequestExt};
use muon::{App, Client, GET};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new client builder.
    let app = App::new("windows-vpn@4.1.0")?.with_user_agent("Mozilla/5.0");
    let store = muon::env::EnvId::new_atlas();
    let builder = Client::builder(app, store);

    // Create the default retry policy for the client.
    let policy = RetryPolicy::default()
        .max_count(3)
        .max_delay(30.s())
        .min_delay(1.s())
        .jitter(500.ms());

    // Build the client with the default retry policy.
    let client = builder.retry_policy(policy).build()?;

    // Set the retry policy for a single request, overriding the default.
    let policy = RetryPolicy::default().max_count(1);

    GET!("/tests/ping")
        .retry_policy(policy)
        .send_with(&client)
        .await?
        .ok()?;

    Ok(())
}
