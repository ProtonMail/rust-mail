use anyhow::{bail, Ok, Result};
use async_trait::async_trait;
use futures::future;
#[allow(deprecated)]
use muon::client::flow::LoginExtraInfo;
use muon::client::flow::LoginFlow;
use muon::client::{Fingerprint, InfoProvider};
use muon::rest::core;
use muon::test::server::{Response, Server};
use muon::util::ProtonRequestExt;
use muon::{GET, POST};
use serde_json::json;
use std::sync::Arc;

#[muon::test(user("foo", "bar"))]
#[allow(deprecated)]
async fn test_auth_deprecated_success(s: Arc<Server>) -> Result<()> {
    let c = s.client();

    let LoginFlow::Ok(c, _) = c
        .auth()
        .login_with_extra("foo", "bar", LoginExtraInfo::default())
        .await
    else {
        bail!("unexpected auth flow");
    };

    let res = GET!("/core/v4/users").send_with(&c).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, "foo");
    assert_eq!(res.user.keys.len(), 1);

    Ok(())
}

#[muon::test(user("foo", "bar"))]
async fn test_auth_success(s: Arc<Server>) -> Result<()> {
    let c = s.client();

    let LoginFlow::Ok(c, _) = c.auth().login("foo", "bar").await else {
        bail!("unexpected auth flow");
    };

    let res = GET!("/core/v4/users").send_with(&c).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, "foo");
    assert_eq!(res.user.keys.len(), 1);

    Ok(())
}

#[muon::test(user("foo", "bar"))]
async fn test_auth_failure(s: Arc<Server>) -> Result<()> {
    assert!(matches!(
        s.client().auth().login("foo", "baz").await,
        LoginFlow::Failed { .. }
    ));

    Ok(())
}

#[muon::test(user("foo", "bar"))]
async fn test_auth_refresh(s: Arc<Server>) -> Result<()> {
    // Authenticate the client.
    let LoginFlow::Ok(c, _) = s.client().auth().login("foo", "bar").await else {
        bail!("unexpected auth flow");
    };

    // Ensure the client is authenticated.
    let res = GET!("/core/v4/users").send_with(&c).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, "foo");

    // Expire the client's session.
    s.expire_user_auth("foo").await?;

    // Perform a request, which should trigger a refresh.
    let res = GET!("/core/v4/users").send_with(&c).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, "foo");

    Ok(())
}

#[muon::test(user("foo", "bar"))]
async fn test_auth_refresh_parallel(s: Arc<Server>) -> Result<()> {
    // Authenticate the client.
    let LoginFlow::Ok(c, _) = s.client().auth().login("foo", "bar").await else {
        bail!("unexpected auth flow");
    };

    // Ensure the client is authenticated by using the GET! macro.
    let res = GET!("/core/v4/users").send_with(&c).await?;
    let res: core::v4::users::GetRes = res.ok()?.into_body_json()?;
    assert_eq!(res.user.name, "foo");

    // Expire the client's session.
    s.expire_user_auth("foo").await?;

    // Begin recording: we want to ensure only one refresh call is made.
    let r = s.new_recorder();

    // Perform 100 parallel requests, which should trigger a refresh.
    future::try_join_all(((0..100).map(|_| c.clone())).map(|c| async move {
        c.send(GET!("/core/v4/users")).await?.ok()?;
        Ok(())
    }))
    .await?;

    // There should be at least 100 `GET /core/v4/users` requests
    // (in reality, closer to 200: most spawned tasks will make two requests,
    // the first will fail due to the expired session, the second will succeed).
    assert!(
        (r.read().iter())
            .filter(|m| m.uri().path() == "/core/v4/users")
            .count()
            >= 100,
    );

    // But only one task should have made a `POST /auth/v4/refresh` request.
    assert_eq!(
        (r.read().iter())
            .filter(|m| m.uri().path() == "/auth/v4/refresh")
            .count(),
        1
    );

    Ok(())
}

#[muon::test]
async fn test_unauth_session_success(s: Arc<Server>) -> Result<()> {
    let fingerprint_content = "Some fingerprint";
    let fingerprint = json!(fingerprint_content);

    let c = s.client().with_info_provider(Arc::new(TestInfoProvider {
        fingerprint: fingerprint.into(),
    }));
    let r = s.new_recorder();

    // Call an endpoint and check if the headers are set
    c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
        .await?;

    // Make sure the unauth session request includes the fingerprint
    let unauth_req = r.read().front().unwrap().to_owned();
    let body_string = String::from_utf8(unauth_req.body().to_vec()).unwrap();
    let trimmed_body_string = &body_string[1..body_string.len() - 1];
    assert!(unauth_req.method() == "POST");
    assert!(unauth_req.uri() == "/auth/v4/sessions");
    assert!(trimmed_body_string == fingerprint_content);

    // Check that the next request has the auth header set
    let req = r.read().pop_back().unwrap();
    let uid_hdr = req.headers().get("x-pm-uid").unwrap();
    let acc_hdr = req.headers().get("authorization").unwrap();
    assert!(!(uid_hdr.to_str()?).is_empty());
    assert!(acc_hdr.to_str()?.contains("Bearer"));

    Ok(())
}

#[muon::test]
async fn test_unauth_session_failure(s: Arc<Server>) -> Result<()> {
    s.add_handler(move |req| (req.uri().path() == "/auth/v4/sessions").then_some(new_res(400)));

    // Call an endpoint and we should get an error back if we are not able to
    // establish a session
    assert!(s
        .client()
        .send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
        .await
        .is_err());

    Ok(())
}

#[muon::test]
async fn test_unauth_session_refresh(s: Arc<Server>) -> Result<()> {
    let c = s.client();
    let r = s.new_recorder();

    // Call an endpoint to make sure we have an unauth session
    c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
        .await?;
    let req = r.read().pop_back().unwrap();
    let uid_hdr = req.headers().get("x-pm-uid").unwrap();
    let acc_hdr = req.headers().get("authorization").unwrap();

    s.expire_all_auth().await?;

    // Call an endpoint to trigger the unauth session refresh
    c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
        .await?;
    let req = r.read().pop_back().unwrap();
    let uid_hdr_refresh = req.headers().get("x-pm-uid").unwrap();
    let acc_hdr_refresh = req.headers().get("authorization").unwrap();

    // UIDs should match. The access tokens should be different
    assert_eq!(uid_hdr.to_str()?, uid_hdr_refresh.to_str()?);
    assert!(acc_hdr.to_str()? != acc_hdr_refresh.to_str()?);

    Ok(())
}

#[muon::test]
async fn test_unauth_session_refresh_failure(s: Arc<Server>) -> Result<()> {
    let c = s.client();
    let r = s.new_recorder();

    // Check all relevant failure codes
    for code in [400, 401, 421, 422] {
        // Call an endpoint to make sure we have an unauth session
        c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
            .await?;
        let req = r.read().pop_back().unwrap();
        let uid_hdr = req.headers().get("x-pm-uid").unwrap();
        let acc_hdr = req.headers().get("authorization").unwrap();

        s.add_handler(move |req| (req.uri().path() == "/auth/v4/refresh").then_some(new_res(code)));
        s.expire_all_auth().await?;

        // Call an endpoint to trigger the unauth session refresh
        c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
            .await?;
        let req = r.read().pop_back().unwrap();
        let uid_hdr_refresh = req.headers().get("x-pm-uid").unwrap();
        let acc_hdr_refresh = req.headers().get("authorization").unwrap();

        // UIDs and access tokens NOT should match. On refresh failure we should create
        // a new unauth session.
        assert_eq!(uid_hdr.to_str()?, uid_hdr_refresh.to_str()?);
        assert!(acc_hdr.to_str()? != acc_hdr_refresh.to_str()?);
    }

    Ok(())
}

#[muon::test]
async fn test_unauth_session_refresh_parallel(s: Arc<Server>) -> Result<()> {
    let c = s.client();
    let r = s.new_recorder();

    // Call an endpoint and check if the headers are set
    c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
        .await?;
    let req = r.read().pop_back().unwrap();
    let uid_hdr = req.headers().get("x-pm-uid").unwrap();
    let acc_hdr = req.headers().get("authorization").unwrap();
    assert!(!(uid_hdr.to_str()?).is_empty());
    assert!(acc_hdr.to_str()?.contains("Bearer"));

    // Expire the client's session.
    s.expire_all_auth().await?;

    // Begin a new recording: we want to ensure only one refresh call is made.
    let r = s.new_recorder();

    // Perform 100 parallel requests, which should trigger a refresh.
    future::try_join_all(((0..100).map(|_| c.clone())).map(|c| async move {
        c.send(POST!("/core/v4/validate/email").body(r#"{"Email":"einstein@pm.me"}"#))
            .await?
            .ok()?;
        Ok(())
    }))
    .await?;

    // There should be at least 100 `POST /core/v4/validate/email` requests
    // (in reality, closer to 200: most spawned tasks will make two requests,
    // the first will fail due to the expired session, the second will succeed).
    assert!(
        (r.read().iter())
            .filter(|m| m.uri().path() == "/core/v4/validate/email")
            .count()
            >= 100,
    );

    // But only one task should have made a `POST /auth/v4/refresh` request.
    assert_eq!(
        (r.read().iter())
            .filter(|m| m.uri().path() == "/auth/v4/refresh")
            .count(),
        1
    );

    Ok(())
}

/// Makes a response with the given status code.
fn new_res<B: Default>(status: u16) -> Response<B> {
    Response::builder()
        .status(status)
        .body(B::default())
        .unwrap()
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
