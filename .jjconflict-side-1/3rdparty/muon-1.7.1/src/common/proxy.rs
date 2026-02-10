//! ## Proxy
//!
//! This module defines types and traits by which a client can determine the
//! proxy to use for a given endpoint.

use crate::common::{Endpoint, IntoDyn, Name};
use muon_proc::{autoimpl, derive_dyn};
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

/// A proxy provider.
///
/// This trait is used to determine the proxy to use for a given endpoint.
/// A proxy provider can be chained with other proxy providers to form a
/// hierarchy of proxy providers.
#[autoimpl(for(DynProxy))]
#[derive_dyn(Debug)]
pub trait Proxy: Send + Sync + 'static {
    /// Return a proxy for the given endpoint.
    fn proxy(&self, endpoint: &Endpoint) -> Option<Endpoint>;
}

/// A dynamic proxy provider.
pub type DynProxy = Arc<dyn Proxy>;

impl<This: Proxy> IntoDyn<DynProxy> for This {
    fn into_dyn(self) -> DynProxy {
        Arc::new(self)
    }
}

impl IntoDyn<DynProxy> for &DynProxy {
    fn into_dyn(self) -> DynProxy {
        self.to_owned()
    }
}

/// Extensions for the [`Proxy`] trait.
#[autoimpl]
pub trait ProxyExt: Proxy + Sized {
    /// Extend this proxy with another proxy.
    ///
    /// This method is used to chain proxy providers together.
    /// If the first proxy provider returns `None`, the next proxy provider is
    /// called, and so on.
    fn chain<T>(self, other: impl IntoIterator<Item = T>) -> DynProxy
    where
        T: IntoDyn<DynProxy>,
    {
        let this = self.into_dyn();

        (other.into_iter())
            .fold(this, |p, o| (p, o.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// A base proxy provider that does not provide a proxy.
#[derive(Debug)]
pub struct BaseProxy;

impl Proxy for BaseProxy {
    fn proxy(&self, _: &Endpoint) -> Option<Endpoint> {
        None
    }
}

/// A proxy provider that looks for a proxy in an environment variable.
/// Depending on the variant, it will apply the proxy to all hosts, only direct
/// hosts, or only indirect hosts.
#[derive(Debug)]
pub struct EnvProxy {
    var: OsString,
    scope: EnvProxyScope,
}

#[derive(Debug)]
enum EnvProxyScope {
    All,
    Loopback,
    External,
}

impl EnvProxy {
    fn new(var: impl AsRef<OsStr>, scope: EnvProxyScope) -> Self {
        let var = var.as_ref().to_owned();

        Self { var, scope }
    }

    /// Create a new environment variable proxy provider for any host.
    #[must_use]
    pub fn all(var: impl AsRef<OsStr>) -> Self {
        Self::new(var, EnvProxyScope::All)
    }

    /// Create a new environment variable proxy provider for loopback hosts.
    #[must_use]
    pub fn loopback(var: impl AsRef<OsStr>) -> Self {
        Self::new(var, EnvProxyScope::Loopback)
    }

    /// Create a new environment variable proxy provider for external hosts.
    #[must_use]
    pub fn external(var: impl AsRef<OsStr>) -> Self {
        Self::new(var, EnvProxyScope::External)
    }

    fn proxy(&self) -> Option<Endpoint> {
        (std::env::var(&self.var).ok())
            .and_then(|var| url!("{var}").ok())
            .and_then(|url| url.try_into().ok())
    }

    fn proxy_loopback(&self, name: &Name) -> Option<Endpoint> {
        if name.is_loopback() {
            self.proxy()
        } else {
            None
        }
    }

    fn proxy_external(&self, name: &Name) -> Option<Endpoint> {
        if name.is_loopback() {
            None
        } else {
            self.proxy()
        }
    }
}

impl Proxy for EnvProxy {
    fn proxy(&self, endpoint: &Endpoint) -> Option<Endpoint> {
        match self.scope {
            EnvProxyScope::All => self.proxy(),
            EnvProxyScope::Loopback => self.proxy_loopback(endpoint.name()),
            EnvProxyScope::External => self.proxy_external(endpoint.name()),
        }
    }
}

/// A proxy provider that always returns the same endpoint.
#[derive(Debug)]
pub struct ConstProxy(Endpoint);

impl ConstProxy {
    /// Create a new constant proxy provider.
    #[must_use]
    pub fn new(endpoint: Endpoint) -> Self {
        Self(endpoint)
    }
}

impl Proxy for ConstProxy {
    fn proxy(&self, _: &Endpoint) -> Option<Endpoint> {
        Some(self.0.clone())
    }
}

impl<L: Proxy, R: Proxy> Proxy for (L, R) {
    fn proxy(&self, endpoint: &Endpoint) -> Option<Endpoint> {
        (self.0.proxy(endpoint)).or_else(|| self.1.proxy(endpoint))
    }
}
