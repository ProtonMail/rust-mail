//! Provides cryptography utility for Proton inbox.

pub mod attachment;
pub mod eo;
pub mod keys;
pub mod lock_icon;
pub mod message;
mod utils;

// re-export crypto crate;
pub use proton_crypto_account::proton_crypto;

// re-export account crate;
pub use proton_crypto_account;

// re-export mime crate;
pub use proton_crypto_inbox_mime;
