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
}

use thiserror::Error;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

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

// #[cfg(feature = "uniffi")]
// mod hidden {
//     use proton_api_core::services::proton::common::RemoteId;
//     use crate::services::proton::response_data::{MessageFlags, MimeType};
//
//     // Note: We need to generate at least on uniffi type which includes custom types
//     // declared in this crate or it will lead to linking issues in the binding code.
//     #[derive(uniffi::Record)]
//     struct UniffiGenCustomTypes {
//         pub cid: RemoteId,
//         pub mid: RemoteId,
//         pub eid: RemoteId,
//         pub mime_type: MimeType,
//         pub msg_flags: MessageFlags,
//     }
//
//     uniffi::ffi_converter_forward!(RemoteId, proton_api_core::UniFfiTag, crate::UniFfiTag);
// }
