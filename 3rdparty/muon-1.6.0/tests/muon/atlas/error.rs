use crate::atlas::{new_atlas_store, new_client, PASS, USER};
use anyhow::{bail, Result};
use derive_more::Error;
use futures::prelude::*;
use muon::client::flow::LoginFlow;
use muon::client::middleware::AuthErr;
use muon::client::{Auth, Tokens};
use muon::common::EnvProxy;
use muon::store::Store;
use muon::{App, Client, GET};

#[tokio::test]
async fn test_error_code() -> Result<()> {
    let client = match new_client().auth().login(USER, PASS).await {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };
    client.logout().await;

    Ok(())
}

#[tokio::test]
async fn test_error_auth_session() -> Result<()> {
    // Create the first client.
    let s1 = new_atlas_store();
    let c1 = Client::builder(App::new("android-mail@99.9.40.0-dev").unwrap(), s1.clone())
        .proxy(EnvProxy::all("http_proxy"))
        .build()?;

    // Log the first client in.
    let c1 = match c1.auth().login(USER, PASS).await {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    // Do bad things: duplicate the first client's auth.
    let mut s2 = new_atlas_store();
    let _ = s1.get_auth().then(|auth| s2.set_auth(auth)).await?;

    // Create the second client.
    let c2 = Client::builder(App::new("android-mail@99.9.40.0-dev").unwrap(), s2)
        .proxy(EnvProxy::all("http_proxy"))
        .build()?;

    // Log the first client out.
    c1.logout().await;

    // Try to use the second client.
    let Err(err) = c2.send(GET!("/core/v4/users")).await else {
        bail!("unexpected success");
    };

    // Get the source of the error.
    let Some(src) = err.source() else {
        bail!("source should be present: {err}");
    };

    assert!(src.is::<AuthErr>());
    assert!(matches!(src.downcast_ref(), Some(AuthErr::Session)));

    Ok(())
}

#[tokio::test]
async fn test_error_auth_unauthorized() -> Result<()> {
    // Create the first client.
    let s1 = new_atlas_store();
    let c1 = Client::builder(App::new("android-mail@99.9.40.0-dev").unwrap(), s1.clone())
        .proxy(EnvProxy::all("http_proxy"))
        .build()?;

    // Log the first client in.
    match c1.auth().login(USER, PASS).await {
        LoginFlow::Ok(_, _) => {}
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    // Do bad things: create a client where the auth token will be refreshed.
    let mut s2 = new_atlas_store();

    if let Auth::Internal { user_id, uid, tok } = s1.get_auth().await {
        let tok = Tokens::refresh(tok.ref_tok());
        let _ = s2.set_auth(Auth::internal(user_id, uid, tok)).await?;
    } else {
        bail!("unexpected auth")
    };

    // Create the second client.
    let c2 = Client::builder(App::new("android-mail@99.9.40.0-dev").unwrap(), s2)
        .proxy(EnvProxy::all("http_proxy"))
        .build()?;

    // Use the second client to trigger an auth refresh.
    let _ = c2.send(GET!("/core/v4/users")).await?.ok()?;

    // Create the third client using the already-refreshed auth.
    let c3 = Client::builder(App::default(), s1.clone())
        .proxy(EnvProxy::all("http_proxy"))
        .build()?;

    // Try to use the third client.
    let Err(err) = c3.send(GET!("/core/v4/users")).await else {
        bail!("unexpected success");
    };

    // Get the source of the error.
    let Some(src) = err.source() else {
        bail!("source should be present: {err}");
    };

    assert!(src.is::<AuthErr>());
    assert!(matches!(src.downcast_ref(), Some(AuthErr::Session)));

    Ok(())
}
