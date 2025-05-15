//! Rust bindings for the REST API for Proton

pub mod services;

pub const MAX_PAGE_ELEMENT_COUNT: usize = 200;
pub const MAX_PAGE_ELEMENT_COUNT_U64: u64 = 200;

pub const MAX_LIMIT_VALUE: usize = 150;
pub const MAX_LIMIT_VALUE_U64: u64 = 150;

pub const INCOMING_DEFAULTS_PAGE_SIZE: u64 = 100;

pub use proton_core_api;

#[cfg(test)]
mod integration_tests {
    use bytes as _;
    use reqwest as _;
    use tokio as _;
    use tracing as _;
    use tracing_subscriber as _;
    use wiremock as _;
}
