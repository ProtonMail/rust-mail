//! Request child data structures for the Proton Mail API.
//!
//! This module provides child data types that are used by the request
//! structures when sending requests to the Proton Mail API.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint request
//! definitions, and NOT have any business logic or other functionality.
//!
//! Structs in this module should only implement [`Serialize`], and should not
//! implement [`Deserialize`](serde::Deserialize). If anything in this module
//! implements [`Deserialize`](serde::Deserialize), it is a sign that a mistake
//! has been made.
//!
//! Any types that used by both requests and responses should be defined in the
//! [`common`](crate::services::proton::common) module.
//!

use crate::services::proton::response_data::MimeType;
use proton_api_core::services::proton::common::RemoteId;
use proton_crypto_inbox::attachment::KeyPackets;
use proton_crypto_inbox::message::EncryptedDraft;
use serde::Serialize;
use serde_repr::Serialize_repr;
use serde_with::{serde_as, BoolFromInt};
use std::collections::HashMap;
//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Serialize, Eq, Hash, PartialEq)]
pub enum MessageMetadataSortMode {
    /// TODO: Document this variant.
    Time,

    /// TODO: Document this variant.
    Size,

    /// TODO: Document this variant.
    ID,
}

/// Draft action.
#[derive(Copy, Clone, Debug, Serialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum DraftAction {
    /// Reply to sender.
    Reply = 0,
    /// Reply to sender and CC list.
    ReplyAll = 1,
    /// Forward to antoher recipient.
    Forward = 2,
}

/// Represents an email address.
#[derive(Clone, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DraftSender {
    /// Email component.
    pub address: String,
    /// Display name if any
    pub name: String,
}

/// Represents a recipient email address.
#[derive(Clone, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DraftRecipient {
    /// Email component.
    pub address: String,
    /// Display name if any
    pub name: String,
    /// Group if any.
    pub group: Option<String>,
}

/// Parameters required to create a new draft.
#[serde_as]
#[derive(Clone, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DraftParams {
    /// Message subject
    pub subject: String,

    /// Whether message is unread.
    #[serde_as(as = "BoolFromInt")]
    pub unread: bool,

    /// Sender of the message.
    pub sender: DraftSender,

    /// To recipients.
    pub to_list: Vec<DraftRecipient>,

    /// CC recipients.
    pub cc_list: Vec<DraftRecipient>,

    /// BCC recipients.
    pub bcc_list: Vec<DraftRecipient>,

    /// External message id to identify the message between mail servers.
    pub external_id: Option<String>,

    /// Bitmap of draft flags
    ///  * Receipt request = 65536 (2^16)
    ///  * Public key = 131072 (2^17)
    ///  * Sign = 262144 (2^18)
    pub draft_flags: u32,

    /// Encrypted message body.
    pub body: EncryptedDraft,

    /// Body mime type
    pub mime_type: MimeType,
}

pub type DraftAttachmentKeyPackets = HashMap<RemoteId, KeyPackets>;
