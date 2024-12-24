#![allow(unreachable_code)]

//! Rust bindings for the REST API for Proton

pub mod auth;
pub mod consts;
pub mod crypto_clock;
pub mod human_verification;
pub mod login;
pub mod service;
pub mod services;
pub mod session;
pub mod store;
pub use services::proton::common::RemoteId;

pub const MAX_PAGE_ELEMENT_COUNT: usize = 200;
pub const SYNC_CONTACT_PAGE_SIZE: usize = 1000;
