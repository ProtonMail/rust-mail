//! Common types used by the Proton Mail API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!

use proton_core_api::declare_proton_id;

declare_proton_id! {
    pub AttachmentId
}
declare_proton_id! {
    pub MessageId
}
declare_proton_id! {
    pub ConversationId
}
declare_proton_id! {
    pub ExternalId
}
