//! Common features of the core domain, such user session management and per user settings.
pub mod cache;
mod context;
pub mod datatypes;
pub mod db;
mod event_subscriber;
pub mod events;
pub mod models;
pub mod os;
mod session;
mod user_context;

pub use context::*;
pub use event_subscriber::*;
pub use session::*;
pub use user_context::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

// #[cfg(feature = "uniffi")]
// mod hidden {
//     use crate::datatypes::RemoteId;
//
//     uniffi::ffi_converter_forward!(
//         RemoteId,
//         proton_api_core::UniFfiTag,
//         crate::UniFfiTag
//     );
// }

// struct OptionalStash(Option<Stash>);
//
// uniffi::custom_newtype!(OptionalStash, None);
