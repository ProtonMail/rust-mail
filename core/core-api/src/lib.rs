#![allow(clippy::enum_glob_use)]
#![allow(clippy::module_name_repetitions)]
#![allow(unreachable_code)]

//! Rust bindings for the REST API for Proton

pub mod auth;
pub mod connection_status;
pub mod consts;
pub mod crypto_clock;
pub mod service;
pub mod services;
pub mod session;
pub mod store;
pub mod utils;
pub mod verification;

pub const MAX_PAGE_ELEMENT_COUNT: u64 = 200;
pub const SYNC_CONTACT_PAGE_SIZE: u64 = 1000;

pub mod exports {
    pub use muon::common::RetryPolicy;
}
