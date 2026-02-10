//! ## Socket
//!
//! This module defines the [`Socket`] trait and related types.
//!
//! A `Socket` is an async read/write stream. It is implemented by any type that
//! can read and write data asynchronously, such as a TCP stream.

use crate::common::IntoDyn;
use futures::{AsyncRead, AsyncWrite};
use muon_proc::{autoimpl, derive_dyn};

/// An async read/write stream.
#[autoimpl]
#[derive_dyn(Debug)]
pub trait Socket: AsyncRead + AsyncWrite + Send + Unpin + 'static {}

/// A dynamic socket.
pub type DynSocket = Box<dyn Socket>;

impl<This: Socket> IntoDyn<DynSocket> for This {
    fn into_dyn(self) -> DynSocket {
        Box::new(self)
    }
}
