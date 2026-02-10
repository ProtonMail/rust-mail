//! ## Net
//!
//! This module defines types representing network addresses, hosts, endpoints,
//! and so on. Importantly, it distinguishes between direct and indirect hosts,
//! which is useful for handling Proton's "alternative routing" feature.

use crate::autoimpl;
use crate::util::ByteSliceExt;
use derive_more::{AsRef, Deref, Display, FromStr};
use std::borrow::Borrow;
use std::net::IpAddr;
use thiserror::Error;
use url::{ParseError as ParseUrlErr, Url};

/// An error that can occur when parsing a name.
#[derive(Debug, Error)]
#[error("invalid name: {0}")]
pub struct ParseNameErr(String);

/// The name of a remote host.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub struct Name(String);

impl Name {
    /// Returns whether the name is loopback
    /// (e.g. `localhost` or an IP address in the loopback range).
    pub fn is_loopback(&self) -> bool {
        if let Ok(ip) = self.0.parse::<IpAddr>() {
            ip.is_loopback()
        } else {
            self.0 == "localhost"
        }
    }

    fn is_valid_name(name: &str) -> bool {
        name.chars().all(Self::is_valid_char)
    }

    fn is_valid_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '.'
    }
}

impl FromStr for Name {
    type Err = ParseNameErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_owned() {
            name if Self::is_valid_name(&name) => Ok(Self(name)),
            name => Err(ParseNameErr(name)),
        }
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// A host which can be resolved to an IP address.
///
/// This type represents two kinds of hosts:
/// - Direct: can be resolved directly to an IP address,
/// - Indirect: must be resolved via one or more intermediate hosts.
///
/// In the context of the Proton API, the standard API endpoint
/// (e.g. `mail.proton.me`) is a direct host and the "alternative routes"
/// (e.g. `d<base-32-string>.protonpro.xyz`) are indirect hosts.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub enum Host {
    /// A host that can be resolved directly.
    Direct(Name),

    /// A host that must be resolved indirectly.
    Indirect(Name),
}

impl Host {
    /// Create a new direct host.
    ///
    /// # Errors
    ///
    /// Returns an error if the host name is invalid.
    pub fn direct(host: impl AsRef<str>) -> Result<Self, ParseNameErr> {
        host.as_ref().parse().map(Host::Direct)
    }

    /// Create a new indirect host.
    ///
    /// # Errors
    ///
    /// Returns an error if the host name is invalid.
    pub fn indirect(host: impl AsRef<str>) -> Result<Self, ParseNameErr> {
        host.as_ref().parse().map(Host::Indirect)
    }

    /// Return whether the host is direct.
    #[must_use]
    pub fn is_direct(&self) -> bool {
        matches!(self, Host::Direct(_))
    }

    /// Return whether the host is indirect.
    #[must_use]
    pub fn is_indirect(&self) -> bool {
        matches!(self, Host::Indirect(_))
    }

    /// Ensure this host is direct, converting indirect hosts to direct hosts.
    #[must_use]
    pub fn to_direct(&self) -> Option<Self> {
        match self {
            Host::Direct(name) => Some(Host::Direct(name.to_owned())),
            Host::Indirect(name) => Some(Host::Direct(Name(name.to_direct()?))),
        }
    }

    /// Ensure this host is indirect, converting direct hosts to indirect hosts.
    #[must_use]
    pub fn to_indirect(&self) -> Self {
        match self {
            Host::Direct(name) => Host::Indirect(Name(name.to_indirect())),
            Host::Indirect(name) => Host::Indirect(name.to_owned()),
        }
    }

    /// Get the name of the host.
    #[must_use]
    pub fn name(&self) -> &Name {
        match self {
            Host::Direct(name) | Host::Indirect(name) => name,
        }
    }
}

impl AsRef<str> for Host {
    fn as_ref(&self) -> &str {
        self.name().as_ref()
    }
}

impl Deref for Host {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.name()
    }
}

impl Borrow<str> for Host {
    fn borrow(&self) -> &str {
        self.name()
    }
}

/// The resolved address of a host.
///
/// This type represents the resolved address of a host.
/// It includes the resolved name of the host and the IP addresses to which it
/// resolves. In the case of an indirect host, the resolved name may differ from
/// the original name.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
#[display("({name}, {ip})")]
pub struct Addr {
    /// The resolved name of the host.
    pub name: Name,

    /// The IP address of the host.
    pub ip: IpAddr,
}

impl Addr {
    /// Create a new address.
    #[must_use]
    pub fn new(name: Name, ip: IpAddr) -> Self {
        Self { name, ip }
    }
}

/// An error that can occur when parsing a scheme.
#[derive(Debug, Error)]
#[error("invalid scheme: {0}")]
pub struct ParseSchemeErr(String);

/// The scheme used by a server.
#[non_exhaustive]
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// The HTTPS scheme.
    #[display("https")]
    Https,

    /// The HTTP scheme.
    #[display("http")]
    Http,
}

impl FromStr for Scheme {
    type Err = ParseSchemeErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("https") {
            Ok(Scheme::Https)
        } else if s.eq_ignore_ascii_case("http") {
            Ok(Scheme::Http)
        } else {
            Err(ParseSchemeErr(s.to_owned()))
        }
    }
}

/// An endpoint, defined by a scheme, host, and port.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
#[display("{host}")]
pub struct Endpoint {
    /// The scheme of the endpoint.
    pub scheme: Scheme,

    /// The host of the endpoint.
    pub host: Host,

    /// The port of the endpoint.
    pub port: u16,
}

impl Endpoint {
    /// Create a new endpoint.
    #[must_use]
    pub fn new(scheme: Scheme, host: Host, port: u16) -> Self {
        Self { scheme, host, port }
    }

    /// Ensure this endpoint is direct, converting indirect hosts to direct
    /// hosts.
    #[must_use]
    pub fn to_direct(&self) -> Option<Self> {
        Some(Self {
            scheme: self.scheme,
            host: self.host.to_direct()?,
            port: self.port,
        })
    }

    /// Ensure this endpoint is indirect, converting direct hosts to indirect
    /// hosts.
    #[must_use]
    pub fn to_indirect(&self) -> Self {
        Self {
            scheme: self.scheme,
            host: self.host.to_indirect(),
            port: self.port,
        }
    }

    /// Create a new HTTP endpoint with a default port of 80.
    #[must_use]
    pub fn http(host: Host) -> Self {
        Self::new(Scheme::Http, host, 80)
    }

    /// Create a new HTTPS endpoint with a default port of 443.
    #[must_use]
    pub fn https(host: Host) -> Self {
        Self::new(Scheme::Https, host, 443)
    }

    /// Get the name of the endpoint.
    #[must_use]
    pub fn name(&self) -> &Name {
        self.host.name()
    }

    /// Build the base URL of the endpoint from the given resolved name.
    pub fn base(&self, name: &Name) -> Result<Url, ParseUrlErr> {
        let scheme = self.scheme;
        let port = self.port;

        url!("{scheme}://{name}:{port}")
    }
}

/// An error that can occur when parsing an endpoint.
#[derive(Debug, Error)]
pub enum ParseEndpointErr {
    /// The string is not a valid URL.
    #[error("invalid URL: {0}")]
    Url(#[from] ParseUrlErr),

    /// The URL cannot be converted to an endpoint.
    #[error("invalid endpoint: {0}")]
    TryFrom(#[from] TryFromUrlErr),
}

impl FromStr for Endpoint {
    type Err = ParseEndpointErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Url::parse(s)?.try_into()?)
    }
}

/// An error that can occur when converting a URL to an endpoint.
#[derive(Debug, Error)]
pub enum TryFromUrlErr {
    /// The URL is missing a host.
    #[error("missing host in URL")]
    Host,

    /// The URL is missing a port, or the port is non-standard but absent.
    #[error("missing or unknown port in URL")]
    Port,

    /// The name in the URL is invalid.
    #[error("invalid name: {0}")]
    Name(#[from] ParseNameErr),

    /// The scheme in the URL is invalid or unsupported.
    #[error("invalid or unsupported scheme: {0}")]
    Scheme(#[from] ParseSchemeErr),
}

impl TryFrom<Url> for Endpoint {
    type Error = TryFromUrlErr;

    fn try_from(value: Url) -> Result<Self, Self::Error> {
        Endpoint::try_from(&value)
    }
}

impl TryFrom<&Url> for Endpoint {
    type Error = TryFromUrlErr;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let scheme = url.scheme().parse()?;
        let host = Host::direct(url.host_str().ok_or(TryFromUrlErr::Host)?)?;
        let port = url.port_or_known_default().ok_or(TryFromUrlErr::Port)?;

        Ok(Endpoint::new(scheme, host, port))
    }
}

/// A server.
///
/// This type represents a single server in an environment.
/// A client can connect to any of the servers in the environment
/// and should consider all servers as equivalent.
///
/// A server consists of an endpoint (scheme, host, and port) and a base path.
#[must_use]
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
#[display("{endpoint}")]
pub struct Server {
    /// The endpoint of the server.
    pub endpoint: Endpoint,

    /// The path of the server's API.
    ///
    /// This is the base path of the server's API, if any. It is used to
    /// construct the full URL of an API endpoint.
    ///
    /// For example, if the base path is `/api`, then all requests to the server
    /// will be made to URLs of the form `https://example.com:443/api...`.
    pub path: String,
}

impl Server {
    /// Create a new server.
    pub fn new(endpoint: Endpoint, path: impl AsRef<str>) -> Self {
        let path = path.as_ref().to_owned();

        Self { endpoint, path }
    }

    /// Return whether the host is direct.
    #[must_use]
    pub fn is_direct(&self) -> bool {
        self.host().is_direct()
    }

    /// Return whether the host is indirect.
    #[must_use]
    pub fn is_indirect(&self) -> bool {
        self.host().is_indirect()
    }

    /// Ensure this server is direct, making indirect hosts direct.
    #[must_use]
    pub fn to_direct(&self) -> Option<Self> {
        Some(Self {
            endpoint: self.endpoint.to_direct()?,
            path: self.path.clone(),
        })
    }

    /// Ensure this server is indirect, making direct hosts indirect.
    pub fn to_indirect(&self) -> Self {
        Self {
            endpoint: self.endpoint.to_indirect(),
            path: self.path.clone(),
        }
    }

    /// Create a new HTTP server with a default port of 80.
    pub fn http(host: Host, path: impl AsRef<str>) -> Self {
        Self::new(Endpoint::http(host), path)
    }

    /// Create a new HTTPS server with a default port of 443.
    pub fn https(host: Host, path: impl AsRef<str>) -> Self {
        Self::new(Endpoint::https(host), path)
    }

    /// Get the scheme of the server.
    #[must_use]
    pub fn scheme(&self) -> Scheme {
        self.endpoint.scheme
    }

    /// Get the host of the server.
    #[must_use]
    pub fn host(&self) -> &Host {
        &self.endpoint.host
    }

    /// Get the name of the server.
    #[must_use]
    pub fn name(&self) -> &Name {
        self.host().name()
    }

    /// Get the port of the server.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.endpoint.port
    }

    /// Build the base URL of the server from the given resolved name.
    pub fn base(&self, name: &Name) -> Result<Url, ParseUrlErr> {
        self.endpoint.base(name)?.join(&self.path)
    }
}

impl FromStr for Server {
    type Err = ParseEndpointErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Url::parse(s)?.try_into()?)
    }
}

impl TryFrom<Url> for Server {
    type Error = TryFromUrlErr;

    fn try_from(value: Url) -> Result<Self, Self::Error> {
        Server::try_from(&value)
    }
}

impl TryFrom<&Url> for Server {
    type Error = TryFromUrlErr;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let endpoint = url.try_into()?;
        let path = url.path().to_owned();

        Ok(Server { endpoint, path })
    }
}

#[autoimpl]
trait HostExt: AsRef<str> {
    fn to_indirect(&self) -> String {
        format!("d{}.protonpro.xyz", self.as_ref().as_b32())
    }

    fn to_direct(&self) -> Option<String> {
        self.as_ref()
            .strip_prefix("d")?
            .strip_suffix(".protonpro.xyz")?
            .b32_to_string()
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::Scheme::*;
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_endpoint_from_str() -> Result<()> {
        let tests = [
            ("http://foo.com:80", Http, Host::direct("foo.com")?, 80),
            ("http://foo.com", Http, Host::direct("foo.com")?, 80),
            ("https://foo.com:443", Https, Host::direct("foo.com")?, 443),
            ("https://foo.com", Https, Host::direct("foo.com")?, 443),
        ];

        for (input, scheme, host, port) in tests {
            let e: Endpoint = input.parse()?;
            assert_eq!(e.scheme, scheme);
            assert_eq!(e.host, host);
            assert_eq!(e.port, port);
        }

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn test_server_from_str() -> Result<()> {
        let tests = [
            ("http://foo.com:80", Http, Host::direct("foo.com")?, 80, "/"),
            ("http://foo.com:80/api", Http, Host::direct("foo.com")?, 80, "/api"),
            ("http://foo.com", Http, Host::direct("foo.com")?, 80, "/"),
            ("http://foo.com/api", Http, Host::direct("foo.com")?, 80, "/api"),
            ("https://foo.com:443", Https, Host::direct("foo.com")?, 443, "/"),
            ("https://foo.com:443/api", Https, Host::direct("foo.com")?, 443, "/api"),
            ("https://foo.com", Https, Host::direct("foo.com")?, 443, "/"),
            ("https://foo.com/api", Https, Host::direct("foo.com")?, 443, "/api"),
        ];

        for (input, scheme, host, port, path) in tests {
            let s: Server = input.parse()?;
            assert_eq!(s.scheme(), scheme);
            assert_eq!(s.host(), &host);
            assert_eq!(s.port(), port);
            assert_eq!(s.path, path);
        }

        Ok(())
    }

    #[test]
    fn test_host_to_indirect() {
        let host = Host::direct("mail-api.proton.me").unwrap();
        let want = Host::indirect("dNVQWS3BNMFYGSLTQOJXXI33OFZWWK.protonpro.xyz").unwrap();

        assert_eq!(host.to_indirect(), want);
    }

    #[test]
    fn test_host_to_direct() {
        let host = Host::indirect("dNVQWS3BNMFYGSLTQOJXXI33OFZWWK.protonpro.xyz").unwrap();
        let want = Host::direct("mail-api.proton.me").unwrap();

        assert_eq!(host.to_direct(), Some(want));
    }

    #[test]
    fn test_host_ext() {
        for host in ["mail.proton.me", "verify.proton.me"] {
            assert_eq!(host.to_indirect().to_direct().unwrap(), host);
        }
    }
}
