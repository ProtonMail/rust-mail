//! # Muon Test
//!
//! This crate implements testing utilities for the `muon` crate.

#[macro_use]
mod cfg;

/// Random stuff.
pub mod rand;

/// Test store implementation.
pub mod store;

/// Matcher utilities for server expectations.
pub mod matcher;

if_tinyproxy! {
    /// Runs a `tinyproxy` instance.
    pub mod proxy;
}

if_runner! {
    /// Test runner harness.
    pub mod runner;
}

if_server! {
    /// Local test server.
    pub mod server;
}
