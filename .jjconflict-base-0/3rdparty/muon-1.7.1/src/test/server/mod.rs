use super::server::backend::Backend;
use super::server::certs::TestTrustAnchor;
use super::server::env::TestEnv;
use super::server::handlers::*;
use super::server::recorder::{Recorder, Rx};
use super::server::responder::Responder;
use super::store::TestStore;
use crate::common::{Host, IntoDyn};
use crate::deps::url::Url;
use crate::env::{Env, EnvId};
use crate::tls::{ParseCert, TlsPin, TlsPinSet};
use crate::util::ByteSliceExt;
use crate::{App, Client};
use anyhow::{anyhow, Result};
use async_channel::{unbounded, Receiver, Sender};
use axum::body::Body;
use axum::routing::{get, post};
use axum::Router;
use derive_more::Debug;
use futures::prelude::*;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use muon_proc::autoimpl;
use rcgen::{Certificate, KeyPair};
use std::io::Error as IoError;
use std::net::{Ipv4Addr, TcpListener as StdTcpListener};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_rustls::server::TlsStream;
use tokio_rustls::{rustls, TlsAcceptor};
use tracing::{debug, error, info};

/// Recorders of all incoming and outgoing messages.
mod recorder;

/// Responders to specific requests.
mod responder;

/// Server backend.
mod backend;

/// Server middleware.
mod middleware;

/// Route handlers.
mod handlers;

/// Server errors.
mod error;

/// Server self-signed certificate generation.
mod certs;

/// Implements a custom API [`Env`].
mod env;

/// The request type.
pub type Request<T = Body> = axum::extract::Request<T>;

/// The response type.
pub type Response<T = Body> = axum::response::Response<T>;

/// The scheme of an HTTP URL.
pub type Scheme = crate::http::Scheme;

/// The HTTP scheme.
pub const HTTP: Scheme = Scheme::HTTP;

/// The HTTPS scheme.
pub const HTTPS: Scheme = Scheme::HTTPS;

/// The server's config.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    prob_429: f64,
    prob_5xx: f64,
}

/// A server that can be used to run tests against.
#[derive(Debug)]
pub struct Server {
    be: Backend,
    url: Url,

    #[debug(skip)]
    ca: Certificate,

    #[debug(skip)]
    cert: Certificate,

    #[debug(skip)]
    key: KeyPair,

    cfg: Arc<Mutex<Config>>,
    rec: Recorder,
    res: Responder,

    task: Mutex<Option<JoinHandle<Result<()>>>>,
    stop: Sender<()>,
}

impl Server {
    /// Runs a new server listening on a random port.
    ///
    /// # Errors
    ///
    /// Returns an error if a random port cannot be bound to, or if the
    /// self-signed certificates cannot be generated.
    pub fn new(rt: &Handle, scheme: &Scheme) -> Result<Arc<Self>> {
        info!("binding to random port");
        let bind = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = bind.local_addr()?;

        debug!("generating CA certificate");
        let (ca, key) = certs::generate_ca()?;

        debug!("generating server certificate");
        let (cert, key) = certs::generate_cert(&ca, &key)?;

        // Create the stop signal.
        let (stop, done) = unbounded();

        let this = Arc::new(Self {
            be: Backend::default(),
            url: url!("{scheme}://{addr}")?,

            ca,
            cert,
            key,

            cfg: Arc::default(),
            rec: Recorder::default(),
            res: Responder::default(),

            task: Mutex::default(),
            stop,
        });

        let task = if scheme == &Scheme::HTTP {
            rt.spawn(this.clone().run_tcp(bind, done))
        } else {
            rt.spawn(this.clone().run_tls(bind, done))
        };

        if let Ok(mut this) = this.task.lock() {
            this.replace(task);
        } else {
            panic!("task lock poisoned")
        }

        Ok(this)
    }

    /// Create a new muon client for this server.
    ///
    /// This is a convenience function that automatically configures the
    /// client's environment to target this server.
    ///
    /// # Panics
    ///
    /// Panics if the client cannot be built.
    pub fn client(&self) -> Client {
        Self::client_for(self, App::default())
    }

    /// Create a new muon client for this server for the given app.
    pub fn client_for(&self, app: App) -> Client {
        self.builder_for(app)
            .build()
            .expect("client should be built")
    }

    /// Create a new muon client builder for this server.
    ///
    /// This is a convenience function that automatically configures the
    /// client's environment to target this server.
    ///
    /// # Panics
    ///
    /// Panics if the client builder cannot be built.
    pub fn builder(&self) -> crate::Builder {
        self.builder_for(App::default())
    }

    /// Create a new muon client builder for this server for the given app.
    pub fn builder_for(&self, app: App) -> crate::Builder {
        let env = EnvId::Custom(self.env().into_dyn());
        let store = TestStore::new(env);
        let anchor = TestTrustAnchor::from_der(self.ca.der());

        Client::builder(app, store).anchor(anchor)
    }

    /// Gets the server's base URL.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the server's host.
    ///
    /// # Panics
    ///
    /// Panics if the server's URL is invalid.
    #[must_use]
    pub fn host(&self) -> Host {
        if let Some(host) = self.url.host_str() {
            Host::direct(host).expect("host must be valid")
        } else {
            panic!("host must be present")
        }
    }

    /// Get the server's CA certificate as a PEM-encoded string.
    #[must_use]
    pub fn ca(&self) -> String {
        self.ca.pem()
    }

    /// Creates an `Env` configured for this server.
    ///
    /// # Panics
    ///
    /// Panics if the server's certificate is invalid.
    pub fn env(&self) -> impl Env {
        let try_env = |this: &Server| -> Result<TestEnv> {
            let cert = this.cert.der().parse_der()?;
            let spki = cert.public_key();
            let pins = TlsPinSet::new([TlsPin::new(spki.raw.sha256())]);
            let base = this.url().try_into()?;

            Ok(TestEnv::new([(base, Some(pins))]))
        };

        try_env(self).expect("server certificate should be valid")
    }

    /// Adds a user to the server's backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the user cannot be added.
    pub async fn new_user(&self, name: &str, pass: &str) -> Result<()> {
        self.be.new_user(name, pass).await?;

        Ok(())
    }

    /// Expires a user's session.
    ///
    /// # Errors
    ///
    /// Returns an error if the user does not exist.
    pub async fn expire_user_auth(&self, name: &str) -> Result<()> {
        (self.be.expire_auth(self.be.get_user_id(name).await?)).await?;

        Ok(())
    }

    /// Expires a all sessions.
    pub async fn expire_all_auth(&self) -> Result<()> {
        self.be.expire_all().await?;

        Ok(())
    }

    /// Adds a new recorder to the server, returning its handle.
    ///
    /// The recorder will record all incoming messages until either the handle
    /// goes out of scope or the server is stopped.
    #[must_use]
    pub fn new_recorder(&self) -> Arc<Rx> {
        self.rec.new_recorder()
    }

    /// Add a new handler to the server.
    ///
    /// The handler will be called for every request.
    /// If it returns `Some`, the response will be sent to the client,
    /// otherwise the next handler will be called, if any.
    /// If no handler returns `Some`, the standard `axum` router will be used.
    pub fn add_handler<F>(&self, handler: F)
    where
        F: Fn(&Request) -> Option<Response> + Send + Sync + 'static,
    {
        self.res.push(Box::new(handler));
    }

    /// Set the probability of a 429 response.
    pub fn set_prob_429(&self, prob: f64) {
        if let Ok(mut cfg) = self.cfg.lock() {
            cfg.prob_429 = prob;
        }
    }

    /// Set the probability of a 5xx response.
    pub fn set_prob_5xx(&self, prob: f64) {
        if let Ok(mut cfg) = self.cfg.lock() {
            cfg.prob_5xx = prob;
        }
    }

    /// Stops the server, waiting for all connections to close.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be stopped.
    pub async fn stop(self: Arc<Self>) -> Result<()> {
        let true = self.stop.close() else {
            return Ok(());
        };

        let Some(task) = self.task.lock().ok().and_then(|mut lock| lock.take()) else {
            return Ok(());
        };

        task.await?
    }

    async fn run_tcp(self: Arc<Self>, bind: StdTcpListener, done: Receiver<()>) -> Result<()> {
        info!("servicing TCP connections on {}", bind.local_addr()?);

        let stream = bind.into_tokio()?.into_stream().with_cancel(done);

        self.serve(stream.boxed()).await
    }

    async fn run_tls(self: Arc<Self>, bind: StdTcpListener, done: Receiver<()>) -> Result<()> {
        info!("servicing TLS connections on {}", bind.local_addr()?);

        let config = rustls::ServerConfig::builder_with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .with_no_client_auth()
            .with_single_cert(
                vec![self.cert.der().to_owned()],
                self.key
                    .serialize_der()
                    .try_into()
                    .map_err(anyhow::Error::msg)?,
            )?;

        let acceptor = TlsAcceptor::from(Arc::new(config));

        let stream = bind
            .into_tokio()?
            .into_stream()
            .and_then(move |io| acceptor.clone().accept_once(io))
            .with_cancel(done);
        info!("serving");
        self.serve(stream.boxed()).await?;
        Ok(())
    }

    async fn serve<I, S>(self: Arc<Self>, mut io: I) -> Result<()>
    where
        I: TryStream<Ok = S, Error = IoError> + Unpin,
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let router = self.router();

        while let Some(sock) = io.try_next().await? {
            debug!("handling I/O stream");

            let sock = TokioIo::new(sock);
            let exec = TokioExecutor::new();
            let svc = TowerToHyperService::new(router.clone());

            tokio::spawn(async move {
                match Builder::new(exec).serve_connection(sock, svc).await {
                    Ok(()) => debug!("connection closed"),
                    Err(e) => error!("connection error: {e}"),
                }
            });
        }

        Ok(())
    }

    #[rustfmt::skip]
    fn router(self: &Arc<Self>) -> Router {
        let unauth = Router::new()
            .route("/auth/v4", post(auth::v4::post))
            .route("/auth/v4/info", post(auth::v4::info::post))
            .route("/auth/v4/refresh", post(auth::v4::refresh::post))
            .route("/auth/v4/sessions", post(auth::v4::sessions::post))
            .route("/tests/ping", get(tests::ping::get))
            .route("/muon/bench", post(crate::test::server::muon::bench::post));

        let auth = Router::new()
            .route("/core/v4/users", get(core::v4::users::get))
            .route("/core/v4/validate/email", post(core::v4::validate::email::post))
            .layer(middleware::auth(&self.be));

        Router::new()
            .merge(unauth)
            .merge(auth)
            .layer(middleware::responder(&self.res))
            .layer(middleware::recorder(&self.rec))
            .layer(middleware::failer(&self.cfg))
            .with_state(self.be.clone())
    }
}

#[autoimpl]
trait IntoTokio: Into<StdTcpListener> + Sized {
    fn into_tokio(self) -> Result<TcpListener> {
        let this = self.into();

        this.set_nonblocking(true)?;

        Ok(TcpListener::from_std(this)?)
    }
}

#[autoimpl]
trait IntoStream: Into<TcpListener> + Sized {
    fn into_stream(self) -> impl Stream<Item = Result<TcpStream, IoError>> {
        stream::unfold(self.into(), |s| async { Some((s.accept().await, s)) }).map_ok(|(io, _)| io)
    }
}

#[autoimpl]
trait AcceptOnce<S>: Into<TlsAcceptor> + Sized
where
    S: AsyncRead + AsyncWrite + Send + Unpin,
{
    fn accept_once<'a>(self, io: S) -> impl Future<Output = Result<TlsStream<S>, IoError>> + 'a
    where
        Self: 'a,
        S: 'a,
    {
        debug!("accepting TLS stream");

        Box::pin(async move {
            let this = self.into();

            match this.accept(io).await {
                Ok(s) => Ok(s),
                Err(e) => Err(IoError::other(anyhow!("accept error: {e}"))),
            }
        })
    }
}

#[autoimpl]
trait WithCancel: Stream + Sized + Send + 'static {
    fn with_cancel(self, rx: Receiver<()>) -> impl Stream<Item = Self::Item> {
        stream::unfold((self.boxed(), rx), |(mut io, rx)| async {
            tokio::select! {
                s = io.next() => s.map(|s| (s, (io, rx))),
                _ = rx.recv() => None,
            }
        })
    }
}
