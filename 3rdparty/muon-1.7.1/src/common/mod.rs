//! ## Muon Common
//!
//! This module defines a core set of types and traits used throughout the rest
//! of the `muon` crate. These are intended to be flexible and reusable building
//! blocks for building higher-level components.

/// The common prelude: just re-exports everything.
pub mod prelude {
    pub use super::*;
}

export! {
    mod connector (as pub);
    mod net (as pub);
    mod policy (as pub);
    mod proxy (as pub);
    mod sender (as pub);
    mod socket (as pub);
    mod timeout (as pub);
    mod traits (as pub);
    mod types (as pub);
}
