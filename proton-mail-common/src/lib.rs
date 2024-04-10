//! Everything Proton Mailbox related.
mod actions;
mod context;
pub mod db;
mod mailbox;
mod user_context;

pub use context::*;
pub use mailbox::*;
pub use user_context::*;

// re-exports
pub use proton_api_mail;
pub use proton_core_common;

pub mod exports {
    pub use proton_action_queue;
    pub use proton_api_mail;
    pub use proton_api_mail::exports::*;
    pub use proton_core_common;
    pub use proton_event_loop;
}

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
