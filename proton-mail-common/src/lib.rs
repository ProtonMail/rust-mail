//! Everything Proton Mailbox related.

pub mod actions;
mod context;
pub mod db;
mod mailbox;

pub mod avatar;

pub mod datatypes;
mod events;
pub mod models;
mod proton_color;
mod user_context;

pub use context::*;
pub use mailbox::*;
pub use user_context::*;

// re-exports
use crate::datatypes::LabelType;
use proton_api_core::service::ApiServiceError;
pub use proton_api_mail;
pub use proton_core_common;
use stash::stash::StashError;

pub mod exports {
    pub use proton_action_queue;
    pub use proton_api_mail;
    pub use proton_core_common;
    pub use proton_event_loop;
    pub use proton_mail_html_transformer;
}

use thiserror::Error;

pub const ALL_LABEL_TYPES: [LabelType; 4] = [
    LabelType::Label,
    LabelType::ContactGroup,
    LabelType::Folder,
    LabelType::System,
];

/// Errors that may occur while using the ProtonMail app.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
    #[error("Other error: {0}")]
    Other(String),
}
