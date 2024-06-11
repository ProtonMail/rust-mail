//! Rust bindings for the REST API for Proton

#[macro_use]
pub mod utils;

pub mod auth;
mod crypto_clock;
pub mod domain;
pub mod exports;
pub mod http;
pub mod login;
pub mod requests;
mod session;

pub use session::*;

pub use requests::APIErrorDesc;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
mod hidden {
    // At least one export with the custom types needs to happen or they will not be resolved
    // in the generated code.
    #[derive(uniffi::Record)]
    struct Dummy {
        pub user_id: crate::domain::UserId,
        pub uid: crate::domain::Uid,
        pub aid: crate::domain::AddressId,
    }
}
