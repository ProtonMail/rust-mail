#[cfg(any(test, debug_assertions))]
pub mod account;
#[cfg(any(test, debug_assertions))]
pub mod addresses;
#[cfg(any(test, debug_assertions))]
pub mod addresses_public;
#[cfg(any(test, debug_assertions))]
pub mod contacts;
#[cfg(any(test, debug_assertions))]
pub mod images_logo;
#[cfg(any(test, debug_assertions))]
pub mod test_context;

#[cfg(any(test, debug_assertions))]
pub mod utils;

/// We have a cyclic dependency:
/// ```ignore
/// proton-core-common v0.9.0 (/core/core-common)
/// ├── proton-core-test-utils v0.1.0 (/core/core-test-utils)
/// │   [dev-dependencies]
/// │   ├── proton-core-common v0.9.0 (/core/core-common) (*)
/// ```
///
/// This reexport allows us to break the chain
#[cfg(any(test, debug_assertions))]
pub mod reexport {
    pub use proton_core_common;
}
