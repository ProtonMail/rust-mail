//! Rust bindings for the REST API for Proton

#[macro_use]
pub mod utils;

pub mod auth;
pub mod client;
pub mod domain;
pub mod exports;
pub mod http;
mod requests;
#[cfg(feature = "uniffi")]
pub mod uniffi_bindgen;

pub use client::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
