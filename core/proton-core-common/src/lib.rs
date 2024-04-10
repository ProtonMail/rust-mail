//! Common features of the core domain, such user session management and per user settings.
mod context;
pub mod db;
mod event_subscriber;
pub mod os;
mod session;
mod user_context;

pub use context::*;
pub use session::*;
pub use user_context::*;

pub use event_subscriber::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
