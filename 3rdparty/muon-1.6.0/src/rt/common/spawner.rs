//! ## Spawner
//!
//! This module defines the [`Spawner`] trait and related types.
//! A spawner is a type that can spawn a future for background execution.
//! The `muon` client uses a spawner to drive any asynchronous tasks that it
//! needs to perform.

use crate::common::IntoDyn;
use futures::future::BoxFuture;
use futures::prelude::*;
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A type capable of spawning a future.
#[autoimpl(for(DynSpawner))]
#[derive_dyn(Debug)]
pub trait Spawner: Send + Sync + 'static {
    /// Spawn the given boxed future.
    fn spawn(&self, fut: BoxFuture<'static, ()>);
}

/// An extension trait for the `Spawner` trait.
#[autoimpl]
pub trait SpawnerExt: Spawner {
    /// Spawn the given future.
    fn spawn_any(&self, fut: impl Future + Send + 'static) {
        self.spawn(Box::pin(fut.map(|_| ())));
    }
}

/// A dynamic spawner; the underlying type is erased.
pub type DynSpawner = Arc<dyn Spawner>;

impl<This: Spawner> IntoDyn<DynSpawner> for This {
    fn into_dyn(self) -> DynSpawner {
        Arc::new(self)
    }
}

impl IntoDyn<DynSpawner> for &DynSpawner {
    fn into_dyn(self) -> DynSpawner {
        self.to_owned()
    }
}
