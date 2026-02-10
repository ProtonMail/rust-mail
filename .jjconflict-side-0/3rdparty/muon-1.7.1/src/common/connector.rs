//! ## Connector
//!
//! A connector is responsible for constructing a [`Sender`] that can be used to
//! send messages to a [`Server`]. This module defines the [`Connector`] trait
//! and related types.
//!
//! [`Connector`]: crate::common::Connector
//! [`Sender`]: crate::common::Sender

use crate::common::{DynSender, IntoDyn, Server};
use crate::Result;
use async_trait::async_trait;
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A type capable of connecting to a server, returning a sender.
#[async_trait]
#[autoimpl(for(DynConnector<T, U>))]
#[derive_dyn(Debug)]
pub trait Connector<T: Send + 'static, U: 'static>: Send + Sync + 'static {
    /// Connect to the given server, returning a sender.
    ///
    /// # Errors
    ///
    /// Returns an error if the server could not be connected to.
    async fn connect(&self, server: &Server) -> Result<DynSender<T, U>>;
}

/// A dynamic connector; the underlying type is erased.
pub type DynConnector<T, U> = Arc<dyn Connector<T, U>>;

impl<This, T: Send + 'static, U: 'static> IntoDyn<DynConnector<T, U>> for This
where
    This: Connector<T, U>,
{
    fn into_dyn(self) -> DynConnector<T, U> {
        Arc::new(self)
    }
}

/// Extensions for the [`Connector`] trait.
#[autoimpl]
pub trait ConnectorExt<T: Send + 'static, U: 'static>: Connector<T, U> + Sized {
    /// Add a layer to the connector.
    fn layer<L>(self, layer: impl IntoIterator<Item = L>) -> DynConnector<T, U>
    where
        L: IntoDyn<DynConnectorLayer<T, U>>,
    {
        let this = self.into_dyn();

        (layer.into_iter())
            .fold(this, |c, l| (c, l.into_dyn()).into_dyn())
            .into_dyn()
    }

    /// Bind the connector to a specific server.
    fn bind(self, server: Server) -> DynBoundConnector<T, U> {
        (self, server).into_dyn()
    }
}

/// A connector layer.
#[async_trait]
#[autoimpl(for(DynConnectorLayer<T, U>))]
#[derive_dyn(Debug)]
pub trait ConnectorLayer<T: Send + 'static, U: 'static>: Send + Sync + 'static {
    /// Connect to the given server using the inner connector.
    ///
    /// # Errors
    ///
    /// Returns an error if the server could not be connected to.
    async fn on_connect(
        &self,
        inner: &dyn Connector<T, U>,
        server: &Server,
    ) -> Result<DynSender<T, U>>;
}

/// A dynamic connector layer.
pub type DynConnectorLayer<T, U> = Arc<dyn ConnectorLayer<T, U>>;

impl<This, T: Send + 'static, U: 'static> IntoDyn<DynConnectorLayer<T, U>> for This
where
    This: ConnectorLayer<T, U>,
{
    fn into_dyn(self) -> DynConnectorLayer<T, U> {
        Arc::new(self)
    }
}

#[async_trait]
impl<C, L, T: Send + 'static, U: 'static> Connector<T, U> for (C, L)
where
    C: Connector<T, U>,
    L: ConnectorLayer<T, U>,
{
    async fn connect(&self, server: &Server) -> Result<DynSender<T, U>> {
        self.1.on_connect(&self.0, server).await
    }
}

/// A bound connector.
#[async_trait]
#[autoimpl(for(DynBoundConnector<T, U>))]
#[derive_dyn(Debug)]
pub trait BoundConnector<T: Send + 'static, U: 'static>: Send + Sync + 'static {
    /// Connect to the server bound to this connector.
    ///
    /// # Errors
    ///
    /// Returns an error if the server could not be connected to.
    async fn connect(&self) -> Result<DynSender<T, U>>;
}

/// A dynamic bound connector.
pub type DynBoundConnector<T, U> = Arc<dyn BoundConnector<T, U>>;

impl<This, T: Send + 'static, U: 'static> IntoDyn<DynBoundConnector<T, U>> for This
where
    This: BoundConnector<T, U>,
{
    fn into_dyn(self) -> DynBoundConnector<T, U> {
        Arc::new(self)
    }
}

#[async_trait]
impl<C, T: Send + 'static, U: 'static> BoundConnector<T, U> for (C, Server)
where
    C: Connector<T, U>,
{
    async fn connect(&self) -> Result<DynSender<T, U>> {
        self.0.connect(&self.1).await
    }
}
