use crate::atlas::{
    invalid_fingerprint, new_client, valid_fingerprint, PASS, PASS_2FA, TOTP_2FA, USER, USER_2FA,
};
use anyhow::{bail, Result};
use async_trait::async_trait;
#[allow(deprecated)]
use muon::client::flow::LoginExtraInfo;
use muon::client::flow::{ForkFlowResult, LoginFlow, WithSelectorFlow};
use muon::client::{Fingerprint, InfoProvider};
use muon::rest::core;
use muon::{Status, GET, POST};
use std::future::Future;
use std::sync::Arc;
use totp_rs::{Algorithm, Secret, TOTP};

#[tokio::test]
async fn test_auth() -> Result<()> {
    _test_auth(new_client().auth().login(USER, PASS)).await
}

#[tokio::test]
async fn test_auth_with_extra() -> Result<()> {
    #[allow(deprecated)]
    _test_auth(
        new_client()
            .auth()
            .login_with_extra(USER, PASS, LoginExtraInfo::default()),
    )
    .await
}

async fn _test_auth(login_flow: impl Future<Output = LoginFlow>) -> Result<()> {
    #[allow(deprecated)]
    let client = match login_flow.await {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    let req = GET!("/core/v4/users");
    let res = client.send(req).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, USER);

    Ok(())
}

#[tokio::test]
#[allow(deprecated)]
async fn test_auth_with_deprecated_fingerprint_api() -> Result<()> {
    let extra_info = LoginExtraInfo::builder()
        .with_fingerprint(valid_fingerprint())
        .build();
    let client = match new_client()
        .auth()
        .login_with_extra(USER, PASS, extra_info)
        .await
    {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    let req = GET!("/core/v4/users");
    let res = client.send(req).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, USER);

    Ok(())
}

#[tokio::test]
async fn test_auth_with_fingerprint_provider() -> Result<()> {
    // Passing an invalid fingerprint will cause the API to return a bad request
    // We do this here to make sure the fingerprint we provide with the provider is
    // used The result of using a valid fingerprint is indistinguishable from
    // using no fingerprint So we use the error as a way to make sure the
    // fingerprint we specify is sent.
    let client = new_client().with_info_provider(Arc::new(TestInfoProvider {
        fingerprint: invalid_fingerprint(),
    }));

    // We expect the login to fail because of the wrong fingerprint
    match client.auth().login(USER, PASS).await {
        LoginFlow::Ok(_, _) => bail!("unexpected success"),
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => {}
    };

    let client = new_client().with_info_provider(Arc::new(TestInfoProvider {
        fingerprint: valid_fingerprint(),
    }));

    // We expect the login to succeed because of the valid fingerprint
    match client.auth().login(USER, PASS).await {
        LoginFlow::Ok(_, _) => {}
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    Ok(())
}

#[tokio::test]
#[allow(deprecated)]
async fn test_auth_with_both_fingerprint_apis() -> Result<()> {
    // If both apis are used, we use the fingerprint passed to LoginExtraInfo for
    // the login call We pass in an invalid fingerprint to LoginExtraInfo and a
    // valid one for the provider If the call fails we can be sure the
    // LoginExtraInfo fingerprint was used
    let extra_info = LoginExtraInfo::builder()
        .with_fingerprint(invalid_fingerprint())
        .build();

    match new_client()
        .with_info_provider(Arc::new(TestInfoProvider {
            fingerprint: valid_fingerprint(),
        }))
        .auth()
        .login_with_extra(USER, PASS, extra_info)
        .await
    {
        LoginFlow::Ok(_, _) => bail!("unexpected success"),
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => {}
    };

    Ok(())
}

#[tokio::test]
async fn test_auth_2fa() -> Result<()> {
    let client = match new_client().auth().login(USER_2FA, PASS_2FA).await {
        LoginFlow::TwoFactor(c, _) => match (c.has_totp(), c.fido_details().is_some()) {
            (true, _) => c.totp(next_totp(TOTP_2FA)?).await?,
            (_, true) => bail!("FIDO not supported"),
            _ => bail!("unexpected 2FA methods"),
        },

        LoginFlow::Ok(_, _) => bail!("unexpected success"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    let req = GET!("/core/v4/users");
    let res = client.send(req).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, USER_2FA);

    Ok(())
}

#[tokio::test]
async fn test_auth_fork() -> Result<()> {
    // Create a new client.
    let c1 = match new_client().auth().login(USER, PASS).await {
        LoginFlow::Ok(c, _) => c,
        LoginFlow::TwoFactor(_, _) => bail!("unexpected TFA flow"),
        LoginFlow::Failed { .. } => bail!("unexpected failure"),
    };

    // Make a fork of the parent session.
    let ForkFlowResult::Success(c1, selector, _session_id) =
        c1.fork("android-mail").payload("foo bar baz").send().await
    else {
        bail!("Failed to fork")
    };

    // Create a new client for the child session.
    let WithSelectorFlow::Ok(c2, _) = new_client()
        .auth()
        .from_fork()
        .with_selector(selector)
        .await
    else {
        bail!("Failed to take ownership of the fork")
    };

    // Both clients should be authorized.
    let req = GET!("/core/v4/users");
    let res = c1.send(req).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, USER);

    let req = GET!("/core/v4/users");
    let res = c2.send(req).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, USER);

    Ok(())
}

fn next_totp(secret: &str) -> Result<String> {
    const DIGITS: usize = 6;
    const SKEW: u8 = 1;
    const STEP: u64 = 30;
    const SHA1: Algorithm = Algorithm::SHA1;

    let secret = Secret::Encoded(secret.to_owned()).to_bytes()?;
    let totp = &TOTP::new(SHA1, DIGITS, SKEW, STEP, secret)?;
    let next = totp.generate_current()?;

    Ok(next)
}

#[tokio::test]
async fn test_unauth_session() -> Result<()> {
    let client = new_client();

    // Call an endpoint that requires an unauth session
    let req = POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#);
    let res = client.send(req).await?;
    let res = res.ok()?;
    assert!(res.is(Status::OK));

    Ok(())
}

#[tokio::test]
async fn test_unauth_session_with_fingerprint() -> Result<()> {
    let client = new_client().with_info_provider(Arc::new(TestInfoProvider {
        fingerprint: valid_fingerprint(),
    }));

    // Call an endpoint that requires an unauth session
    let req = POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#);
    let res = client.send(req).await?;
    let res = res.ok()?;
    assert!(res.is(Status::OK));

    Ok(())
}

#[tokio::test]
async fn test_unauth_session_with_fingerprint_error() -> Result<()> {
    // The backend returns an error for this fingerprint
    // We want to make sure that the backend reacts to our fingerprints
    // That is why a failure case was added
    let client = new_client().with_info_provider(Arc::new(TestInfoProvider {
        fingerprint: invalid_fingerprint(),
    }));

    // Call an endpoint that requires an unauth session
    let req = POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#);
    let Err(_err) = client.send(req).await else {
        bail!("unexpected success");
    };

    Ok(())
}

struct TestInfoProvider {
    fingerprint: Fingerprint,
}

#[async_trait]
impl InfoProvider for TestInfoProvider {
    async fn fingerprint(&self) -> Option<Fingerprint> {
        Some(self.fingerprint.to_owned())
    }
}
