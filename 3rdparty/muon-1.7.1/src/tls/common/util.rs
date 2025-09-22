use crate::common::{DynSocket, Host, IntoDyn, Name, Socket};
use crate::rt::{Dialer, DialerLayer};
use crate::tls::{Alpn, DynTlsUpgrader, TlsUpgrader};
use crate::Result;
use async_trait::async_trait;
use futures::TryFutureExt;
use muon_proc::autoimpl;
use std::borrow::Borrow;
use std::net::SocketAddr;

/// An extension trait for sockets, providing methods for upgrading them to TLS.
#[autoimpl]
#[async_trait]
pub trait TlsSocketExt<T: TlsUpgrader>: Socket + Sized {
    /// Upgrade the socket to TLS with the given ALPN protocols.
    async fn upgrade(
        self,
        tls: &T,
        host: &Host,
        name: &Name,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>)> {
        tls.upgrade(self.into_dyn(), host, name, alpn).await
    }
}

/// Create a layer which upgrades a dialer to support TLS.
pub fn with_upgrader(
    upgrader: impl IntoDyn<DynTlsUpgrader>,
    host: impl Borrow<Host>,
    name: impl Borrow<Name>,
) -> WithUpgraderLayer {
    WithUpgraderLayer::new(upgrader, host, name)
}

/// A layer which upgrades a dialer to support TLS.
#[derive(Debug)]
pub struct WithUpgraderLayer {
    upgrader: DynTlsUpgrader,
    host: Host,
    name: Name,
}

impl WithUpgraderLayer {
    fn new(
        upgrader: impl IntoDyn<DynTlsUpgrader>,
        host: impl Borrow<Host>,
        name: impl Borrow<Name>,
    ) -> Self {
        Self {
            upgrader: upgrader.into_dyn(),
            host: host.borrow().to_owned(),
            name: name.borrow().to_owned(),
        }
    }
}

#[async_trait]
impl DialerLayer for WithUpgraderLayer {
    async fn on_dial(&self, inner: &dyn Dialer, addr: SocketAddr) -> Result<DynSocket> {
        inner
            .dial(addr)
            .await?
            .upgrade(&self.upgrader, &self.host, &self.name, &[])
            .map_ok(|(sock, _)| sock)
            .await
    }
}
