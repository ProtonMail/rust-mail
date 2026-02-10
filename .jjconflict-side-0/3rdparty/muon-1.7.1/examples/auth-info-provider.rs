//! This example demonstrates the basic login flow for a muon client with an
//! info provider The info provider is used by the muon client to fetch the
//! fingerprint and attach it to the login call

use anyhow::{bail, Result};
use async_trait::async_trait;
use muon::client::flow::LoginFlow;
use muon::client::{Fingerprint, InfoProvider};
use muon::json::json;
use muon::{App, Client, GET};
use std::sync::Arc;

// An example implementation of InfoProvider used by the muon Client to fetch
// the fingerprint.
struct ExampleInfoProvider {}

#[async_trait]
impl InfoProvider for ExampleInfoProvider {
    async fn fingerprint(&self) -> Option<Fingerprint> {
        let fingerprint = json!({
            "mail-android-99.9.40.0-challenge":{
                "appLang":"en",
                "deviceName":"TestDevice",
                "frame":{
                    "name":"username"
                },
                "isDarkmodeOn":false,
                "isJailbreak":false,
                "keyboards":[

                ],
                "preferredContentSize":"2.0",
                "regionCode":"CH",
                "storageCapacity":"63.8",
                "timezone":"Europe/Zurich",
                "timezoneOffset":"0",
                "v":"2.0.0"
            }
        })
        .into();
        Some(fingerprint)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // First, define which app is using the client.
    let app = App::new("windows-vpn@4.1.0")?;

    // Set the user agent, if desired.
    let app = app.with_user_agent("Mozilla/5.0");

    // Then, specify where the client will persist its session data. We'll use the
    // TestStore for this example; a real app would implement its own store.
    // A store is tied to a specific environment; a prod store holds prod tokens,
    // an atlas store holds atlas tokens, etc.
    let store = muon::env::EnvId::new_atlas();

    // Finally, create the client. The client will be configured to connect to the
    // prod environment, and the session data will be stored in the TestStore.
    // Additionally an info_provider is attached to the Client.
    // The info_provider is used to fetch the fingerprint from the apps that
    // integrate muon. This fingerprint is attached to the login call to help
    // prevent fraud.
    let client = Client::new(app, store)?.with_info_provider(Arc::new(ExampleInfoProvider {}));

    // Auth stuff is done via the auth flow.
    // To begin, call the auth method on the client.
    let auth = client.auth();

    // We can use the auth flow to login.
    let client = match auth.login("visionary", "a").await {
        // The client is now authenticated,
        // and the tokens are in the store.
        LoginFlow::Ok(client, _) => client,

        // The client needs 2FA to complete the login.
        // We can inspect the client to see what kind of 2FA is available.
        LoginFlow::TwoFactor(client, _) => {
            if client.has_totp() {
                client.totp("123456").await?
            } else if client.fido_details().is_some() {
                unimplemented!()
            } else {
                bail!("no 2FA available");
            }
        }

        LoginFlow::Failed { reason, client } => {
            println!("Login failure: {reason}, client is staying un-logged.");
            client
        }
    };

    // The client can *always* do the ping
    client.send(GET!("/tests/ping")).await?.ok()?;

    // Now we can use the client to make authenticated requests, if the login was
    // successful. The client will automatically use the tokens in the store.
    // If the tokens are expired, the client will refresh them.
    client.send(GET!("/core/v4/users")).await?.ok()?;

    Ok(())
}
