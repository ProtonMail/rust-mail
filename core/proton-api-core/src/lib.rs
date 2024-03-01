//! Rust bindings for the REST API for Proton

#[macro_use]
pub mod utils;

pub mod auth;
pub mod domain;
pub mod exports;
pub mod http;
pub mod login;
mod requests;
mod session;

pub use session::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
