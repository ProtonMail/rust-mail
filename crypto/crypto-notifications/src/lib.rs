//! This library provides Business Unit agnostic code for decrypting Push Notifications.
//!  

// re-export crypto crate;
pub use proton_crypto_account::proton_crypto;

// re-export account crate;
pub use proton_crypto_account;

mod decrypt;

pub use decrypt::*;
