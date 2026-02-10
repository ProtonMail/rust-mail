//! ## Dialer
//!
//! This module defines the [`Dialer`] trait and related types.
//!
//! A `Dialer` is a type capable of dialing remote hosts.
//! Given a list of IP addresses and a port, a `Dialer` attempts to connect to
//! one of them on the given port, returning a `Socket` if successful.

use crate::common::{DynSocket, IntoDyn};
use crate::Result;
use async_trait::async_trait;
use muon_proc::{autoimpl, derive_dyn};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// A type capable of dialing remote hosts.
#[async_trait]
#[autoimpl(for(DynDialer))]
#[derive_dyn(Debug)]
pub trait Dialer: Send + Sync + 'static {
    /// Open a connection to the given socket address.
    async fn dial(&self, addr: SocketAddr) -> Result<DynSocket>;

    /// Dial the given address/port pair.
    async fn dial_addr(&self, addr: IpAddr, port: u16) -> Result<DynSocket> {
        self.dial((addr, port).into()).await
    }
}

/// A dynamic dialer.
pub type DynDialer = Arc<dyn Dialer>;

impl<This: Dialer> IntoDyn<DynDialer> for This {
    fn into_dyn(self) -> DynDialer {
        Arc::new(self)
    }
}

impl IntoDyn<DynDialer> for &DynDialer {
    fn into_dyn(self) -> DynDialer {
        self.to_owned()
    }
}

/// An extension trait for the `Dialer` trait.
#[autoimpl]
pub trait DialerExt: Dialer + Sized {
    /// Add a layer to the dialer.
    fn layer<L>(self, layer: impl IntoIterator<Item = L>) -> DynDialer
    where
        L: IntoDyn<DynDialerLayer>,
    {
        let this = self.into_dyn();

        (layer.into_iter())
            .fold(this, |d, l| (d, l.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// A dialer layer.
#[async_trait]
#[autoimpl(for(DynDialerLayer))]
#[derive_dyn(Debug)]
pub trait DialerLayer: Send + Sync + 'static {
    /// Dial the given address using the inner dialer.
    async fn on_dial(&self, inner: &dyn Dialer, addr: SocketAddr) -> Result<DynSocket>;
}

/// A dynamic dialer layer.
pub type DynDialerLayer = Arc<dyn DialerLayer>;

impl<This: DialerLayer> IntoDyn<DynDialerLayer> for This {
    fn into_dyn(self) -> DynDialerLayer {
        Arc::new(self)
    }
}

impl IntoDyn<DynDialerLayer> for &DynDialerLayer {
    fn into_dyn(self) -> DynDialerLayer {
        self.to_owned()
    }
}

#[async_trait]
impl<D, L> Dialer for (D, L)
where
    D: Dialer,
    L: DialerLayer,
{
    async fn dial(&self, addr: SocketAddr) -> Result<DynSocket> {
        self.1.on_dial(&self.0, addr).await
    }
}
