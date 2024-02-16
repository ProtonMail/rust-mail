//! Time related async features.

#[cfg(feature = "tokio-time")]
mod tokio_time;
#[cfg(feature = "tokio-time")]
pub use tokio_time::*;
