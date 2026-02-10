#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod cancellation;
pub mod document;
pub mod engine;
pub mod entry;
pub mod index;
pub mod processor;
pub mod query;
pub mod serialization;
pub mod transaction;

/// Cantor pairing utilities for unique EntryIndex generation
mod cantor_pairing;
mod chunker;
#[cfg(feature = "wasm-bindgen")]
pub mod setup;
/// WAL utilities for search engine operations
mod wal_utils;

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
