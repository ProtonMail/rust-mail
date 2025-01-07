//! Common types used by the Proton Mail API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!

use proton_api_core::declare_proton_id;
use serde_repr::{Deserialize_repr, Serialize_repr};

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq, Serialize_repr)]
#[repr(u8)]
pub enum LabelType {
    /// TODO: Document this variant.
    Label = 1,

    /// TODO: Document this variant.
    ContactGroup = 2,

    /// TODO: Document this variant.
    Folder = 3,

    /// TODO: Document this variant.
    System = 4,
}

declare_proton_id!(
    /// Identifier for a proton Attachment.
    pub AttachmentId
);

declare_proton_id!(
    /// Identifier for a proton Messages.
    pub MessageId
);

declare_proton_id!(
    /// Identifier for a proton Conversation.
    pub ConversationId
);
