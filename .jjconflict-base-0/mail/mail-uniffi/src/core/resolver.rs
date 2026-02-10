use std::net::IpAddr as StdIpAddr;
use std::sync::Arc;

use muon::Result as MuonResult;
use muon::common::{Addr, Host, Name};
use muon::rt::{ResolveRes as MuonResolveRes, Resolver as MuonResolver};
use muon::util::IntoIterExt;

#[derive(uniffi::Enum)]
pub enum IpAddr {
    V4(String),
    V6(String),
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ResolverError {
    #[error("Failed to resolve due to lack of network")]
    Network(String),
    #[error("{0}")]
    Other(String),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ResolverError {
    fn from(value: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Other(value.to_string())
    }
}

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve the given host to a set of IP addresses.
    async fn resolve(&self, host: String) -> Result<Option<Vec<IpAddr>>, ResolverError>;
}

impl std::fmt::Debug for dyn Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Resolver")
    }
}

#[derive(Debug)]
pub struct ResolverImpl(Arc<dyn Resolver>);

impl ResolverImpl {
    pub fn new(resolver: Arc<dyn Resolver>) -> Self {
        Self(resolver)
    }

    async fn resolve_direct(&self, name: &Name) -> MuonResult<MuonResolveRes> {
        let mut res = Vec::new();

        let addrs = match self.0.resolve(name.to_string()).await {
            Ok(None) => return Ok(MuonResolveRes::None),
            Ok(Some(addrs)) => addrs,
            Err(e) => {
                return Err(muon::Error::other(Box::new(e)));
            }
        };

        for addr in addrs {
            match addr {
                IpAddr::V4(addr) => {
                    if let Ok(addr) = addr.parse() {
                        res.push(Addr::new(name.to_owned(), StdIpAddr::V4(addr)));
                    }
                }

                IpAddr::V6(addr) => {
                    if let Ok(addr) = addr.parse() {
                        res.push(Addr::new(name.to_owned(), StdIpAddr::V6(addr)));
                    }
                }
            }
        }

        if let Some((head, tail)) = res.into_head_tail() {
            Ok(MuonResolveRes::Some(head, tail.collect()))
        } else {
            Ok(MuonResolveRes::None)
        }
    }
}

#[async_trait::async_trait]
impl MuonResolver for ResolverImpl {
    async fn resolve(&self, host: &Host) -> MuonResult<MuonResolveRes> {
        if let Host::Direct(name) = host {
            self.resolve_direct(name).await
        } else {
            Ok(MuonResolveRes::None)
        }
    }
}
