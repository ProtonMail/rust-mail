use anyhow::Result;
use muon::common::RetryPolicy;
use muon::test::server::{Response, Server};
use muon::GET;
use std::sync::Arc;

#[muon::test]
async fn test_handle_429_once(s: Arc<Server>) -> Result<()> {
    let p = RetryPolicy::default().max_count(2);
    let c = s.client();

    // Succeeds on third request (second retry).
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(429)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(429)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(200)));
    assert!(c.send(GET!("/foo").retry_policy(p)).await?.ok().is_ok());

    Ok(())
}

#[muon::test]
async fn test_handle_429(s: Arc<Server>) -> Result<()> {
    let p = RetryPolicy::default().max_count(1);
    let r = s.new_recorder();
    let c = s.client();

    // Succeeds on second request (first retry).
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(429)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(200)));
    assert!(c.send(GET!("/foo").retry_policy(p)).await?.ok().is_ok());

    // Should have made three requests (unauth session + original + retry).
    assert_eq!(r.take().len(), 3);

    // Would succeed on third request (second retry), but fails due to retry limit.
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(429)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(429)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(200)));
    assert!(c.send(GET!("/foo").retry_policy(p)).await?.ok().is_err());

    // Should have made two requests (original + retry).
    // Unauth session request is made for the first request only.
    assert_eq!(r.take().len(), 2);

    Ok(())
}

#[muon::test]
async fn test_handle_5xx(s: Arc<Server>) -> Result<()> {
    let p = RetryPolicy::default().max_count(1);
    let r = s.new_recorder();
    let c = s.client();

    // Succeeds on second request (first retry).
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(503)));
    s.add_handler(|req| (req.uri().path() == "/foo").then_some(new_res(200)));
    assert!(c.send(GET!("/foo").retry_policy(p)).await?.ok().is_ok());

    // Should have made three requests (unauth session + original + retry).
    assert_eq!(r.take().len(), 3);

    Ok(())
}

/// Makes a response with the given status code.
fn new_res<B: Default>(status: u16) -> Response<B> {
    Response::builder()
        .status(status)
        .body(B::default())
        .unwrap()
}
