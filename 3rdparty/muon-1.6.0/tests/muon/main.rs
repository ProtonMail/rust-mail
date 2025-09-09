#![cfg(not(target_family = "wasm"))]

//! # Muon Tests

/// Tests against the local fake API.
#[cfg(feature = "test-local")]
mod local;

/// Tests against the atlas API.
#[cfg(feature = "test-atlas")]
mod atlas;

/// DNS tests.
#[cfg(feature = "test-dns")]
mod dns;

/// DNS-over-HTTPS tests.
#[cfg(feature = "test-doh")]
mod doh;

/// Initialization.
mod init;
