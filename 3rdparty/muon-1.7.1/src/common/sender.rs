//! ## Sender
//!
//! A sender is something that can send requests and receive responses.
//! This module provides the `Sender` trait, which is implemented by types that
//! can send requests, as well as the `SenderLayer` trait, which is implemented
//! by types that can add additional functionality to a sender.

use crate::common::{BoxFut, IntoDyn};
use crate::Result;
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A type capable of sending requests.
#[autoimpl(for(DynSender<T, U>))]
#[derive_dyn(Debug)]
pub trait Sender<T: Send + 'static, U: 'static>: Send + Sync + 'static {
    /// Send the given request and return the response.
    fn send(&self, req: T) -> BoxFut<'_, Result<U>>;
}

/// A dynamic sender; the underlying type is erased.
pub type DynSender<T, U> = Arc<dyn Sender<T, U>>;

impl<This, T: Send + 'static, U: 'static> IntoDyn<DynSender<T, U>> for This
where
    This: Sender<T, U>,
{
    fn into_dyn(self) -> DynSender<T, U> {
        Arc::new(self)
    }
}

/// An extension trait for the `Sender` trait.
#[autoimpl]
pub trait SenderExt<T: Send + 'static, U: 'static>: Sender<T, U> + Sized {
    /// Add a layer to the sender.
    fn layer<L>(self, layer: impl IntoIterator<Item = L>) -> DynSender<T, U>
    where
        L: IntoDyn<DynSenderLayer<T, U>>,
    {
        let this = self.into_dyn();

        (layer.into_iter())
            .fold(this, |s, l| (s, l.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// A sender layer: wraps a sender to add additional functionality.
#[autoimpl(for(DynSenderLayer<T, U>))]
#[derive_dyn(Debug)]
pub trait SenderLayer<T: Send + 'static, U: 'static>: Send + Sync + 'static {
    /// Forward the request to an inner sender.
    fn on_send<'a>(&'a self, inner: &'a dyn Sender<T, U>, req: T) -> BoxFut<'a, Result<U>>;
}

/// A dynamic sender layer.
pub type DynSenderLayer<T, U> = Arc<dyn SenderLayer<T, U>>;

impl<This, T: Send + 'static, U: 'static> IntoDyn<DynSenderLayer<T, U>> for This
where
    This: SenderLayer<T, U>,
{
    fn into_dyn(self) -> DynSenderLayer<T, U> {
        Arc::new(self)
    }
}

impl<S, L, T: Send + 'static, U: 'static> Sender<T, U> for (S, L)
where
    S: Sender<T, U>,
    L: SenderLayer<T, U>,
{
    fn send(&self, req: T) -> BoxFut<'_, Result<U>> {
        Box::pin(self.1.on_send(&self.0, req))
    }
}
