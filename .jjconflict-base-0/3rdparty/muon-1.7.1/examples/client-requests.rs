//! This example demonstrates how to build and send requests with a muon client.

use anyhow::{bail, Result};
use muon::client::flow::LoginFlow;
use muon::common::RetryPolicy;
use muon::util::{DurationExt, ProtonRequestExt};
use muon::{App, Client, ContentType, Method, ProtonRequest, GET, POST};
use serde_json::json;

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

    // The first way to build a request is with one of the various macros.
    let _ = GET!("/core/v4/users");
    let _ = POST!("/auth/v4/info");

    // The macros support interpolation.
    let id = 123;
    let _ = GET!("/mail/v4/messages/{}", 123);
    let _ = GET!("/mail/v4/messages/{arg}", arg = 123);
    let _ = GET!("/mail/v4/messages/{id}");

    // Headers and query parameters can be added to the request like so.
    let _ = GET!("/core/v4/users")
        .query(("limit", 10))
        .query(("limit", String::from("foo")))
        .query(("flag",))
        .header(("X-Test", "test"))
        .header(ContentType::JSON);

    // Raw bodies and JSON bodies can be added to the request.
    let _ = POST!("/core/v4/users").body(b"hello world");
    let _ = POST!("/core/v4/users").body_json(json!({"name": "bob"}));

    // Per-request policies can be set.
    let _ = GET!("/core/v4/users").retry_policy(
        RetryPolicy::default()
            .min_delay(500.ms())
            .max_delay(10.s())
            .iter_add(250.us())
            .iter_mul(2.0),
    );

    // Arbitrary-typed objects can be set and retrieved from the request,
    // useful for layers and middleware.
    let req = GET!("/core/v4/users")
        .extension(Foo(123))
        .extension(Bar("hello".to_string()));

    assert_eq!(req.get_extension(), Some(&Foo(123)));
    assert_eq!(req.get_extension(), Some(&Bar("hello".to_string())));

    // You can also build a request directly from the `HttpReq` type.
    let _ = ProtonRequest::new(Method::GET, "/tests/ping")
        .header(ContentType::JSON)
        .body(r#"{"ping": "pong"}"#);

    // Once built, a request can be passed to the client to be sent.
    let _ = client.send(GET!("/tests/ping")).await?;

    // Alternatively, a client can be passed to the request to be sent.
    let _ = GET!("/tests/ping").send_with(&client).await?;

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Foo(u32);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bar(String);
