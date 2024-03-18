//! Provides cryptography utility for Proton inbox.

pub mod attachment;
pub mod keys;
mod utils;

// re-export crypto crate;
pub use proton_crypto;
