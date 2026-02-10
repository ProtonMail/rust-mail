use crate::http::Status;
use crate::test::server::backend::Backend;
use crate::test::server::recorder::Recorder;
use crate::test::server::responder::Responder;
use crate::test::server::Config;
use axum::body::{Body, HttpBody};
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use futures::future::{self, BoxFuture};
use muon_proc::autoimpl;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Create a new auth layer with the given backend.
pub fn auth(be: &Backend) -> AuthLayer {
    AuthLayer { be: be.to_owned() }
}

/// Middleware that authenticates a request.
#[derive(Debug, Clone)]
pub struct AuthLayer {
    be: Backend,
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthSvc<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthSvc {
            be: self.be.clone(),
            inner,
        }
    }
}

/// Authenticates a request: check the headers for `x-pm-uid` and
/// `Authorization`, and if they are present, validate them.
/// On success, the request is passed to the next middleware,
/// with the user ID and scopes attached to the request.
#[derive(Debug, Clone)]
pub struct AuthSvc<S> {
    be: Backend,
    inner: S,
}

impl<S> AuthSvc<S> {
    async fn try_verify(&self, mut req: Request) -> Option<Request> {
        let uid = req.headers().get("x-pm-uid")?.to_str().ok()?;
        let tok = req.headers().get("authorization")?.to_str().ok()?;
        let tok = tok.strip_prefix("Bearer ")?;

        let (user_id, scopes) = self.be.verify_auth(uid, tok).await.ok()?;

        req.extensions_mut().insert(user_id);
        req.extensions_mut().insert(scopes);

        Some(req)
    }
}

impl<S: HttpSvc> Service<Request> for AuthSvc<S>
where
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut this = self.to_owned();

        Box::pin(async move {
            if let Some(req) = this.try_verify(req).await {
                this.inner.call(req).await
            } else {
                Ok(Status::UNAUTHORIZED.into_response())
            }
        })
    }
}

/// Create a new recorder layer with the given recorder.
pub fn recorder(rec: &Recorder) -> RecorderLayer {
    RecorderLayer {
        rec: rec.to_owned(),
    }
}

/// Middleware that authenticates a request.
#[derive(Debug, Clone)]
pub struct RecorderLayer {
    rec: Recorder,
}

impl<S> Layer<S> for RecorderLayer {
    type Service = RecorderSvc<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RecorderSvc {
            rec: self.rec.clone(),
            inner,
        }
    }
}

/// Authenticates a request: check the headers for `x-pm-uid` and
/// `Authorization`, and if they are present, validate them.
/// On success, the request is passed to the next middleware,
/// with the user ID and scopes attached to the request.
#[derive(Debug, Clone)]
pub struct RecorderSvc<S> {
    rec: Recorder,
    inner: S,
}

impl<S: HttpSvc> Service<Request> for RecorderSvc<S>
where
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut this = self.to_owned();

        Box::pin(async move {
            // Split the request into parts and body bytes.
            let (p, mut b) = req.into_parts();

            // Collect all the request body bytes.
            let Ok(b) = collect(&mut b).await else {
                return Ok(Status::INTERNAL_SERVER_ERROR.into_response());
            };

            // Record a clone of the request.
            this.rec.push(&Request::from_parts(p.clone(), b.clone()));

            // Rebuild the original request (need to re-create a collectable body).
            this.inner.call(Request::from_parts(p, b.into())).await
        })
    }
}

async fn collect(body: &mut Body) -> Result<Vec<u8>, axum::Error> {
    let mut body = pin!(body);
    let mut bufs = Vec::new();

    while let Some(data) = future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        if let Ok(data) = data?.into_data() {
            bufs.push(data);
        }
    }

    Ok(bufs.concat())
}

/// Create a new responder layer with the given responder.
///
/// A responder enables setting custom responses for any matching requests.
pub fn responder(res: &Responder) -> ResponderLayer {
    ResponderLayer {
        res: res.to_owned(),
    }
}

/// A responder layer.
#[derive(Debug, Clone)]
pub struct ResponderLayer {
    res: Responder,
}

impl<S> Layer<S> for ResponderLayer {
    type Service = ResponderSvc<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ResponderSvc {
            res: self.res.clone(),
            inner,
        }
    }
}

/// Middleware that responds to requests.
#[derive(Debug, Clone)]
pub struct ResponderSvc<S> {
    res: Responder,
    inner: S,
}

impl<S: HttpSvc> Service<Request> for ResponderSvc<S>
where
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut this = self.to_owned();

        Box::pin(async move {
            if let Some(res) = this.res.get(&req) {
                Ok(res)
            } else {
                this.inner.call(req).await
            }
        })
    }
}

/// Create a new layer that randomly returns 429 responses.
pub fn failer(cfg: &Arc<Mutex<Config>>) -> RateLimiterLayer {
    RateLimiterLayer(cfg.to_owned())
}

/// A rate limiter layer.
#[derive(Debug, Clone)]
pub struct RateLimiterLayer(Arc<Mutex<Config>>);

impl<S> Layer<S> for RateLimiterLayer {
    type Service = RateLimiterSvc<S>;

    fn layer(&self, inner: S) -> Self::Service {
        let cfg = self.0.clone();

        RateLimiterSvc { cfg, inner }
    }
}

/// Middleware that randomly returns 429 responses.
#[derive(Debug, Clone)]
pub struct RateLimiterSvc<S> {
    cfg: Arc<Mutex<Config>>,
    inner: S,
}

impl<S: HttpSvc> Service<Request> for RateLimiterSvc<S>
where
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut this = self.to_owned();

        Box::pin(async move {
            let p = rand::random::<f64>();

            if let Ok(cfg) = this.cfg.lock() {
                if p < cfg.prob_429 {
                    return Ok(Status::TOO_MANY_REQUESTS.into_response());
                }

                if p < cfg.prob_429 + cfg.prob_5xx {
                    return Ok(Status::SERVICE_UNAVAILABLE.into_response());
                }
            }

            this.inner.call(req).await
        })
    }
}

/// An alias for a thread-safe service.
#[autoimpl]
trait SafeSvc<T, U>: Service<T, Response = U>
where
    Self: Clone + Send + Sync + 'static,
    Self::Future: Send,
{
}

/// An alias for a thread-safe HTTP service.
#[autoimpl]
trait HttpSvc: SafeSvc<Request, Response>
where
    Self::Future: Send,
{
}
