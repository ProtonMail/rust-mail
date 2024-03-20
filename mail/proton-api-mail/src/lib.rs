//! Rust bindings for the REST API for Proton

pub mod domain;
mod requests;
mod session;

pub mod exports {
    pub use proton_api_core::exports::*;
}

pub use proton_api_core;
pub use session::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
