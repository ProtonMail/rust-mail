//! Rust bindings for the REST API for Proton

pub mod domain;
pub mod requests;
mod session;

pub mod exports {
    pub use proton_api_core::exports::*;
}

pub use proton_api_core;
pub use session::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
mod hidden {
    use crate::domain::{ConversationId, ExternalId, MessageId};

    // Note: We need to generate at least on uniffi type which includes custom types
    // declared in this crate or it will lead to linking issues in the binding code.
    #[derive(uniffi::Record)]
    struct UniffiGenCustomTypes {
        pub cid: ConversationId,
        pub mid: MessageId,
        pub eid: ExternalId,
    }
}
