use crate::deps::url::Url;
use anyhow::{bail, Result};
use itertools::Itertools;
use lazy_static::lazy_static;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::Write;
use std::net::{Ipv4Addr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tempfile::{NamedTempFile, TempPath};
use tracing::debug;

lazy_static! {
    /// A global `tinyproxy` instance.
    static ref PROXY: Mutex<Option<TinyProxy>> = Mutex::new(None);
}

/// Get the URL of the global `tinyproxy` instance.
///
/// # Errors
///
/// Returns an error if no proxy is running and it cannot be started.
pub fn url() -> Result<Url> {
    // Lock the global proxy.
    let Ok(mut guard) = PROXY.lock() else {
        bail!("lock poisoned");
    };

    // If the proxy is not running, start it.
    if guard.is_none() {
        *guard = Some(TinyProxy::builder().build()?);
    }

    // Get the proxy's URL.
    if let Some(proxy) = guard.as_ref() {
        Ok(proxy.url().clone())
    } else {
        unreachable!()
    }
}

/// Builds a new `tinyproxy` instance.
#[must_use]
#[derive(Debug)]
pub struct Builder {
    /// The proxy's host.
    host: Ipv4Addr,

    /// The proxy's port.
    port: u16,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            host: Ipv4Addr::LOCALHOST,
            port: 1111,
        }
    }
}

impl Builder {
    /// Set the proxy's host.
    pub fn host(mut self, host: Ipv4Addr) -> Self {
        self.host = host;
        self
    }

    /// Set the proxy's port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Build the configured `tinyproxy` instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the proxy cannot be started.
    pub fn build(self) -> Result<TinyProxy> {
        let Self { host, port } = self;

        let url = url!("http://{host}:{port}")?;
        let cfg = mkfile(Config::new(host, port))?;
        let res = TinyProxy::new(url, cfg)?;

        while (TcpStream::connect((host, port))).is_err() {
            debug!("waiting for tinyproxy to start");
        }

        Ok(res)
    }
}

/// Holds a `tinyproxy` process, killing it when dropped.
#[derive(Debug)]
pub struct TinyProxy {
    /// The `tinyproxy` process.
    child: Child,

    /// The proxy's address.
    url: Url,

    /// The proxy's config file.
    cfg: TempPath,
}

impl TinyProxy {
    fn new(url: Url, cfg: TempPath) -> Result<Self> {
        let child = Command::new("tinyproxy")
            .arg("-c")
            .arg(&cfg)
            .arg("-d")
            .stdout(Stdio::null())
            .spawn()?;

        Ok(Self { child, url, cfg })
    }

    /// Create a new builder.
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Get the proxy's address.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the proxy's process ID.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Display for TinyProxy {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "tinyproxy({})", self.pid())
    }
}

/// Kill the child when dropped.
impl Drop for TinyProxy {
    fn drop(&mut self) {
        debug!(pid = %self.pid(), %self.url, ?self.cfg, "stopping tinyproxy");

        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// A representation of the `tinyproxy` config file.
#[derive(Debug)]
struct Config {
    /// The proxy's host.
    host: Ipv4Addr,

    /// The proxy's port.
    port: u16,
}

impl Config {
    fn new(host: Ipv4Addr, port: u16) -> Self {
        Self { host, port }
    }
}

impl IntoIterator for Config {
    type Item = String;
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.render().into_iter())
    }
}

impl Config {
    /// Render the config file to a string.
    fn render(&self) -> impl IntoIterator<Item = String> {
        [
            format!("Listen {}", self.host),
            format!("Port {}", self.port),
        ]
    }
}

/// Create a new named temp file with the given content.
fn mkfile(data: impl IntoIterator<Item = String>) -> Result<TempPath> {
    let mut file = NamedTempFile::new()?;

    file.write_all(data.into_iter().join("\n").as_bytes())?;

    Ok(file.into_temp_path())
}
