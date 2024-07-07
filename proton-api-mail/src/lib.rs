//! Rust bindings for the REST API for Proton

pub mod services;
pub mod session;

pub const MAX_PAGE_ELEMENT_COUNT: usize = 200;
pub const MAX_PAGE_ELEMENT_COUNT_U64: u64 = 200;

pub const MAX_LIMIT_VALUE: usize = 150;
pub const MAX_LIMIT_VALUE_U64: u64 = 150;

pub use proton_api_core;
pub use session::*;
