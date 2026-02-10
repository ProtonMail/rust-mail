//! ## HTTP Request
//!
//! This module implements an HTTP request type.
//! It uses the builder pattern to prepare a request for sending.
//!
//! Once prepared, a crate-internal [`HttpReq::build`] method is used to convert
//! the request into a [`http::Request<Body>`]; this is not part of the public
//! API, ensuring we are free to change the internal representation of the
//! request if needed.

use crate::common::prelude::*;
use crate::http::common::util::JsonExt;
use crate::http::{Body, ContentType, HttpRes};
use crate::util::With;
use crate::{ErrorKind, Result};
use common_multipart_rfc7578::client::multipart::{self, Form};
use derive_more::Display;
use futures::TryStreamExt;
use http::header::HOST;
use http::request::Builder;
use itertools::Itertools;
use muon_proc::autoimpl;
use serde::Serialize;
use std::borrow::Borrow;
use std::fmt::{Formatter, Result as FmtResult};
use std::future::Future;
use std::time::Duration;
use url::Url;

/// Represents an HTTP version.
pub type Version = http::Version;

/// Represents an HTTP scheme.
pub type Scheme = http::uri::Scheme;

/// An HTTP method.
///
/// TODO: Add more? Depends what API actually uses and what we support.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// A `GET` request.
    GET,

    /// A `POST` request.
    POST,

    /// A `PUT` request.
    PUT,

    /// A `DELETE` request.
    DELETE,

    /// A `PATCH` request.
    PATCH,
}

impl From<Method> for http::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::GET => Self::GET,
            Method::POST => Self::POST,
            Method::PUT => Self::PUT,
            Method::DELETE => Self::DELETE,
            Method::PATCH => Self::PATCH,
        }
    }
}

/// A constraint of time for a request
#[derive(Debug, Clone, PartialEq, Eq)]
struct TimeConstraint {
    /// the constraint per-se
    time_constraint: Duration,

    /// true if this object was built via [`TimeConstraint::default()`]
    is_default: bool,
}

impl<T> From<T> for TimeConstraint
where
    T: Into<Duration>,
{
    fn from(time_constraint: T) -> Self {
        let time_constraint = time_constraint.into();
        Self {
            time_constraint,
            is_default: false,
        }
    }
}

impl AsRef<Duration> for TimeConstraint {
    fn as_ref(&self) -> &Duration {
        &self.time_constraint
    }
}

impl Default for TimeConstraint {
    fn default() -> Self {
        Self {
            time_constraint: std::time::Duration::from_secs(30),
            is_default: true,
        }
    }
}

/// An HTTP request.
#[must_use]
#[derive(Debug, Clone)]
pub struct HttpReq {
    /// The HTTP method.
    method: Method,

    /// The servers to send this request to.
    /// Made up of direct and indirect servers (for alternative routing).
    servers: Option<Vec<Server>>,

    /// The path segments of the request.
    path: Vec<String>,

    /// The query parameters of the request.
    query: Vec<(String, Option<String>)>,

    /// The headers of the request.
    header: Vec<(String, String)>,

    /// The body of the request.
    body: Vec<u8>,

    /// The retry policy of the request.
    retry: Option<RetryPolicy>,

    /// Time constraint for this request.
    time_constraint: TimeConstraint,

    /// The service type of the request.
    service: Option<ServiceType>,

    /// An idempotent request can be send via multiple transports
    /// simultaneously, while a non-idempotent one will be run in a way that
    /// ensures that there's low risk that multiple requests would reach the
    /// backend at the same time.
    idempotent: bool,

    /// The cost of the request.
    cost: Option<ServiceCost>,

    /// Extensions for the request.
    ext: TypeMap,
}

impl HttpReq {
    /// Create a new request with the given method and path.
    pub fn new(method: Method, path: impl AsRef<str>) -> Self {
        Self {
            method,

            servers: None,
            path: path_segs(path),
            query: Vec::new(),
            header: Vec::new(),
            body: Vec::new(),

            retry: None,
            time_constraint: TimeConstraint::default(),
            service: None,

            // by default, we do not take the risk of doing parallel requests
            idempotent: false,

            cost: None,

            ext: TypeMap::default(),
        }
    }

    /// Set the servers this request can be sent to.
    ///
    /// This overrides the servers provided by the [`Env`] used to configure the
    /// client. If set, the client will try to connect to each server in order,
    /// and will send the request to the first server to which it successfully
    /// connects, or fail if no server is reachable. In particular, the client
    /// will not fallback to its default servers.
    ///
    /// Note that a server can be either direct or indirect. Alternative routing
    /// is implemented by configuring the client with both direct and indirect
    /// servers. As such, when overriding the servers, you must provide both
    /// direct and indirect servers in order to use alternative routing.
    ///
    /// [`Env`]: crate::env::Env
    pub fn servers(self, servers: impl IntoIterator<Item = Server>) -> Self {
        let servers = Some(servers.into_iter().collect());

        Self { servers, ..self }
    }

    /// Get the method of the request.
    #[must_use]
    pub fn get_method(&self) -> Method {
        self.method
    }

    /// Get the path segments of the request.
    #[must_use]
    pub fn get_path(&self) -> &[String] {
        &self.path
    }

    /// Add a query to the request.
    pub fn query(self, query: impl AsQuery) -> Self {
        let query = self.query.with_many(query.as_query());

        Self { query, ..self }
    }

    /// Get the query parameters of the request.
    #[must_use]
    pub fn get_query(&self) -> &[(String, Option<String>)] {
        &self.query
    }

    /// Add a header to the request.
    pub fn header(self, header: impl AsHeader) -> Self {
        let header = self.header.with_many(header.as_header());

        Self { header, ..self }
    }

    /// Get the headers of the request.
    #[must_use]
    pub fn get_header(&self) -> &[(String, String)] {
        &self.header
    }

    /// Set the request body.
    pub fn body(self, body: impl AsRef<[u8]>) -> Self {
        let body = body.as_ref().to_owned();

        Self { body, ..self }
    }

    /// Set the request body, serialized as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the body could not be serialized.
    pub fn body_json(self, body: impl Serialize) -> Result<Self> {
        let body = body.encode_json().map_err(ErrorKind::req)?;

        Ok(self.body(body).header(ContentType::JSON))
    }

    /// Get the body of the request.
    #[must_use]
    pub fn get_body(&self) -> &[u8] {
        &self.body
    }

    /// Add a retry policy to the request.
    pub fn retry_policy(self, policy: RetryPolicy) -> Self {
        let retry = Some(policy);

        Self { retry, ..self }
    }

    /// Get the retry policy.
    #[must_use]
    pub fn get_retry_policy(&self) -> Option<&RetryPolicy> {
        self.retry.as_ref()
    }

    /// Add a timeout policy to the request.
    pub fn allowed_time(self, constraint: impl Into<Duration>) -> Self {
        let time_constraint = TimeConstraint::from(constraint);

        Self {
            time_constraint,
            ..self
        }
    }

    /// Get the timeout policy.
    #[must_use]
    pub fn get_allowed_time(&self) -> Duration {
        self.time_constraint.as_ref().to_owned()
    }

    /// Add a service type to the request.
    pub fn service_type(self, service: ServiceType, idempotent: bool) -> Self {
        Self {
            service: Some(service),
            idempotent,
            ..self
        }
    }

    /// Get the service type.
    #[must_use]
    pub fn get_service_type(&self) -> Option<ServiceType> {
        self.service
    }

    /// Get whether the request is idempotent.
    #[must_use]
    pub fn is_indempotent(&self) -> bool {
        self.idempotent
    }

    /// Add a cost to the request.
    pub fn service_cost(self, cost: ServiceCost) -> Self {
        let cost = Some(cost);

        Self { cost, ..self }
    }

    /// Get the service cost.
    #[must_use]
    pub fn get_service_cost(&self) -> Option<ServiceCost> {
        self.cost
    }

    /// Add an extension to the request.
    pub fn extension<T: Clone + Send + Sync + 'static>(mut self, ext: T) -> Self {
        self.ext.insert(ext);

        self
    }

    /// Get an extension from the request.
    #[must_use]
    pub fn get_extension<T: Clone + Send + Sync + 'static>(&self) -> Option<&T> {
        self.ext.get()
    }

    /// Get the timeout controller, if set.
    #[must_use]
    pub fn get_timeout_ctl(&self) -> Option<TimeoutCtl> {
        self.get_extension().cloned()
    }

    /// Get the servers to send this request to.
    /// Made up of direct and indirect servers (for alternative routing).
    pub(crate) fn get_servers(&self) -> Option<&[Server]> {
        self.servers.as_deref()
    }

    /// Build the request with the given HTTP version for the given target.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be built.
    pub(crate) fn build(
        self,
        version: Version,
        server: &Server,
        name: &Name,
    ) -> Result<http::Request<Body>> {
        trace!(?version, %server, %name, "building request");

        self.build_uri(version, server, name)
            .map(|(uri, host)| self.build_req(version, uri, host))?
            .body(Body::new(self.body))
            .map_err(ErrorKind::req)
    }

    fn build_uri(
        &self,
        version: Version,
        server: &Server,
        name: &Name,
    ) -> Result<(String, Option<String>)> {
        let mut uri: Url = server.base(name).map_err(ErrorKind::req)?;

        if let Ok(mut segs) = uri.path_segments_mut() {
            segs.extend(&self.path);
        }

        for (key, val) in &self.query {
            if let Some(val) = val {
                uri.query_pairs_mut().append_pair(key, val);
            } else {
                uri.query_pairs_mut().append_key_only(key);
            }
        }

        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                Ok((uri.to_string(), None))
            } else {
                Ok(if version == Version::HTTP_11 {
                    (trim_base(&uri), uri.host_str().map(String::from))
                } else {
                    (uri.to_string(), None)
                })
            }
        }
    }

    fn build_req(&self, version: Version, uri: String, host: Option<String>) -> Builder {
        let mut req = Builder::new().version(version).method(self.method).uri(uri);

        if let Some(host) = host {
            req = req.header(HOST, host);
        }

        for (key, val) in &self.header {
            req = req.header(key, val);
        }

        req
    }
}

impl Display for HttpReq {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "{} {}", self.method, self.path.join("/"))
    }
}

/// A type that can be converted into a header.
pub trait AsHeader {
    /// Convert the type into a header's key-value pair.
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)>;
}

impl<T: AsHeader> AsHeader for &T {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        (*self).as_header()
    }
}

impl<K: AsRef<str>, V: AsRef<str>> AsHeader for (K, V) {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [(self.0.as_ref().to_owned(), self.1.as_ref().to_owned())]
    }
}

/// A type that can be converted into a query key or key/value pair.
pub trait AsQuery {
    /// Convert the type into a header's key-value pair.
    fn as_query(&self) -> impl IntoIterator<Item = (String, Option<String>)>;
}

impl<T: AsQuery> AsQuery for &T {
    fn as_query(&self) -> impl IntoIterator<Item = (String, Option<String>)> {
        (*self).as_query()
    }
}

impl<K: ToString> AsQuery for (K,) {
    fn as_query(&self) -> impl IntoIterator<Item = (String, Option<String>)> {
        [(self.0.to_string(), None)]
    }
}

impl<K: ToString, V: ToString> AsQuery for (K, V) {
    fn as_query(&self) -> impl IntoIterator<Item = (String, Option<String>)> {
        [(self.0.to_string(), Some(self.1.to_string()))]
    }
}

/// Converts a serializable value into a query.
///
/// # Example
/// ```rust
/// use muon::http::{AsQuery, serde_to_query};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Query {
///     foo: String,
///     bar: u32,
/// }
///
/// let query = Query {
///     foo: "hello".to_owned(),
///     bar: 42,
/// };
///
/// let query: Vec<_> = serde_to_query(query)
///     .unwrap()
///     .as_query()
///     .into_iter()
///     .collect();
///
/// assert_eq!(query, vec![
///     ("foo".to_owned(), Some("hello".to_owned())),
///     ("bar".to_owned(), Some("42".to_owned())),
/// ]);
/// ```
pub fn serde_to_query(value: impl Serialize) -> Result<impl AsQuery, serde_qs::Error> {
    struct AsQueryImpl(String);

    impl AsQuery for AsQueryImpl {
        fn as_query(&self) -> impl IntoIterator<Item = (String, Option<String>)> {
            url::form_urlencoded::parse(self.0.as_bytes())
                .into_owned()
                .collect_vec()
                .into_iter()
                .map(|(key, val)| (key, Some(val)))
        }
    }

    Ok(AsQueryImpl(serde_qs::to_string(&value)?))
}

/// An extension trait for HTTP requests.
#[autoimpl]
pub trait HttpReqExt: Into<HttpReq> + Sized + Send {
    /// Try to send the request using the given sender.
    fn send_with<'a, S>(self, sender: &'a S) -> BoxFut<'a, Result<HttpRes>>
    where
        Self: 'a,
        S: ?Sized + Sender<HttpReq, HttpRes>,
    {
        sender.send(self.into())
    }

    /// Build multipart request.
    ///
    /// It allows to build body with exposed closure, and sets correct content
    /// type.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use muon::GET;
    /// # use muon::Result;
    /// # use crate::muon::http::HttpReqExt;
    /// # use std::io::Cursor;
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    ///     let req = GET!("/tests/ping")
    ///         .multipart(move |mut form| {
    ///             form.add_text("MyText", "1234");
    ///             form.add_reader_file_with_mime(
    ///                 "WhalesSounds",
    ///                 Cursor::new("•၊၊||၊|။||||။၊|။•"),
    ///                 "WhalesSounds.wav",
    ///                 "audio/vnd.wav".parse().unwrap(),
    ///             );
    ///             form
    ///         })
    ///         .await?;
    /// #    Ok(())
    /// # }
    /// ```
    fn multipart(
        self,
        fnform: impl FnOnce(Form<'_>) -> Form<'_> + Send,
    ) -> impl Future<Output = Result<HttpReq>> + Send {
        async move {
            let rq = self.into();
            let form = fnform(Form::default());
            let content_type = form.content_type();
            let body = multipart::Body::from(form)
                .try_concat()
                .await
                .map_err(ErrorKind::req)?;
            let this = rq.header(("Content-type", content_type)).body(body);

            Ok(this)
        }
    }
}

/// Get the segments of the request path.
fn path_segs(path: impl AsRef<str>) -> Vec<String> {
    path.as_ref()
        .trim_start_matches('/')
        .split('/')
        .map(str::to_owned)
        .collect()
}

/// Remove the `http://foo.com` prefix from the URL, leaving just `/bar/baz?qux=quux#corge`.
fn trim_base(url: impl Borrow<Url>) -> String {
    let url: &Url = url.borrow();

    let mut res = url.path().to_owned();

    if let Some(query) = url.query() {
        res.push('?');
        res.push_str(query);
    }

    if let Some(fragment) = url.fragment() {
        res.push('#');
        res.push_str(fragment);
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::GET;
    use anyhow::Result;

    #[test]
    #[rustfmt::skip]
    fn test_uri() -> Result<()> {
        let tests = [
            // --- Simple Base URL ---
            ("https://foo.com", "/", "/", "https://foo.com/"),
            ("https://foo.com", "/bar", "/bar", "https://foo.com/bar"),
            ("https://foo.com", "/bar/", "/bar/", "https://foo.com/bar/"),

            // --- Base URL with Path ---
            ("https://foo.com/api", "/", "/api/", "https://foo.com/api/"),
            ("https://foo.com/api", "/bar", "/api/bar", "https://foo.com/api/bar"),
            ("https://foo.com/api", "/bar/", "/api/bar/", "https://foo.com/api/bar/"),
        ];

        for (base, path, uri1, uri2) in tests {
            let srv = base.parse()?;

            // HTTP/1.1 requests have uri of the form `/some/path`
            let req1 = GET!("{path}").build(Version::HTTP_11, &srv, srv.name())?;
            assert_eq!(req1.uri(), uri1);

            // HTTP/2 requests have uri of the form `https://foo.com/some/path`
            let req2 = GET!("{path}").build(Version::HTTP_2, &srv, srv.name())?;
            assert_eq!(req2.uri(), uri2);
        }

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn test_trim_head() -> Result<()> {
        let tests = [
            // --- Just Path ---
            ("https://foo.com/", "/"),
            ("https://foo.com/bar", "/bar"),
            ("https://foo.com/bar/baz", "/bar/baz"),

            // --- With Query ---
            ("https://foo.com/?query=value", "/?query=value"),
            ("https://foo.com/bar?query=value", "/bar?query=value"),
            ("https://foo.com/bar/baz?query=value&another=thing", "/bar/baz?query=value&another=thing"),

            // --- With Fragment ---
            ("https://foo.com/#fragment", "/#fragment"),
            ("https://foo.com/bar#fragment", "/bar#fragment"),
            ("https://foo.com/bar/baz#fragment", "/bar/baz#fragment"),

            // --- With Query and Fragment ---
            ("https://foo.com/?query=value#fragment", "/?query=value#fragment"),
            ("https://foo.com/bar?query=value#fragment", "/bar?query=value#fragment"),
            ("https://foo.com/bar/baz?query=value&another=thing#fragment", "/bar/baz?query=value&another=thing#fragment"),
        ];

        for (path, want) in tests {
            let lhs = Url::parse(path)?;
            let rhs = trim_base(&lhs);

            assert_eq!(rhs, want);
        }

        Ok(())
    }
}
