//! Rust bindings for the REST API for Proton

#[macro_use]
mod utils;

pub mod clientv2;
pub mod domain;
pub mod exports;
pub mod http;
mod requests;
#[cfg(feature = "uniffi")]
pub mod uniffi_bindgen;

pub use clientv2::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
