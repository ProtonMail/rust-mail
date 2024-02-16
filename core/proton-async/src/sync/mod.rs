//! Wrappers around async primitives.
pub mod mpsc;

#[cfg(feature = "tokio-sync")]
mod tokio_rwlock;
#[cfg(feature = "tokio-sync")]
pub use tokio_rwlock::*;
