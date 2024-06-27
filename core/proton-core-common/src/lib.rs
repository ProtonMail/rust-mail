//! Common features of the core domain, such user session management and per user settings.
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

#[cfg(feature = "uniffi")]
mod hidden {
    uniffi::ffi_converter_forward!(
        proton_api_core::domain::ContactId,
        proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
    uniffi::ffi_converter_forward!(
        proton_api_core::domain::ContactLabelId,
        proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
    uniffi::ffi_converter_forward!(
        proton_api_core::domain::ContactEmailId,
        proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
    uniffi::ffi_converter_forward!(
        proton_api_core::domain::ContactUid,
        proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
}
