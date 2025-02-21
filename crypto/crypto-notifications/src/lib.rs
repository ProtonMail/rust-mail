//! This library provides Business Unit agnostic code for decrypting and veryfing Push Notifications.
//!  

// re-export crypto crate;
pub use proton_crypto_account::proton_crypto;

// re-export account crate;
pub use proton_crypto_account;

mod decrypt;
mod verify;

pub use decrypt::*;
pub use verify::*;
