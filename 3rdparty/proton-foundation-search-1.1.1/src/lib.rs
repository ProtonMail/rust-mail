#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

pub mod blob;
pub mod cancellation;
pub mod document;
pub mod engine;
pub mod entry;
pub mod index;
pub mod processor;
pub mod query;
pub mod serialization;
pub mod svec;

/// Cantor pairing utilities for unique EntryIndex generation
mod chunker;
#[cfg(feature = "wasm-bindgen")]
pub mod setup;

mod prelude {
    pub use alloc::boxed::Box;
    pub use alloc::string::{String, ToString};
    pub use alloc::sync::Arc;
    pub use alloc::vec::Vec;
    pub type BoxError = Box<dyn core::error::Error + Send + Sync>;
}

// Unused, but do not remove!
//
// This function emits a compile-time error if somebody removes/changes
// the expected compile-time filters for the tracing dependency,
// which could lead to leakage of sensitive user data.
#[doc(hidden)]
#[allow(dead_code)]
#[cfg(test)]
fn ensure_proper_compile_time_tracing_filters() {
    #[cfg(all(not(debug_assertions), not(feature = "compile-time-tracing-filters")))]
    compile_error!("the \"compile-time-tracing-filters\" feature is required for release builds!");
}
