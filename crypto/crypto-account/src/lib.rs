//! Wrapper library around cryptography primitives

pub mod domain;
pub mod keyring;
pub mod salts;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

// re-export crypto crate;
pub use proton_crypto;
