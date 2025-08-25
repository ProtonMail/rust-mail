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

use crate::services::proton::common::{AttachmentId, MessageId};
use crate::services::proton::prelude::MobileAction;
use crate::services::proton::response_data::MimeType;
use indexmap::IndexMap;
use proton_core_api::services::proton::{PrivateEmail, PrivateString};
use proton_crypto_inbox::attachment::{
    Base64AttachmentEncryptedSignature, BinaryAttachmentEncryptedSignature,
    BinaryAttachmentSignature, EncryptedAttachment, KeyPackets,
};
use proton_crypto_inbox::keys::{InboxSessionKey, KeyPacket, PackageCryptoType, SessionKeyExposed};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::message::packages::PackageMimeType;
use proton_crypto_inbox::proton_crypto::crypto::SessionKeyAlgorithm;
use serde::Serialize;
use serde_repr::Serialize_repr;
use serde_with::{BoolFromInt, DisplayFromStr, base64::Base64, serde_as};
use std::collections::HashMap;
//  ENUMS
//==============================================================================

#[derive(Clone, Copy, Debug, Serialize, Eq, Hash, PartialEq)]
pub enum MessageMetadataSortMode {
    Time,
    SnoozeTime,
    Size,
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

/// Parameters required to submit when forwarding or replying to a draft message.
pub struct DraftReplyOrForwardParams {
    /// Id of the message we are replying to or forwarding from.
    pub parent_id: MessageId,
    /// Nature of the action.
    pub action: DraftAction,
}

/// Represents an email address.
#[derive(Clone, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DraftSender {
    /// Email component.
    pub address: PrivateEmail,
    /// Display name if any
    pub name: PrivateString,
}

/// Represents a recipient email address.
#[derive(Clone, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DraftRecipient {
    /// Email component.
    pub address: PrivateEmail,
    /// Display name if any
    pub name: PrivateString,
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

    #[serde(rename = "CCList")]
    /// CC recipients.
    pub cc_list: Vec<DraftRecipient>,

    /// BCC recipients.
    #[serde(rename = "BCCList")]
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
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,
}

pub type DraftAttachmentKeyPackets = IndexMap<AttachmentId, KeyPackets>;
pub type PackageAddresses = HashMap<String, AddressSubPackage>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum PackageAttachmentEntries<T> {
    /// Map of attachment remote ids onto `T`s (attachment keys or signatures).
    ///
    /// This is used for building the "send draft" request, because that's the
    /// only case where we know attachment remote ids up-front.
    Draft(HashMap<String, T>),

    /// List of attachment `T`s (attachment keys or signatures).
    ///
    /// This is used for building the "send direct mail" request, because there
    /// we don't know attachment remote ids up front, so we can only rely on
    /// correlating attachments by indices.
    ///
    /// Entries here must be listed in the same order in which attachments
    /// appear within the message.
    Direct(Vec<T>),
}

/// Signature mode of a sub-package.
#[derive(Debug, Default, Serialize_repr, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum PackageSignaturesMode {
    /// No signatures.
    #[default]
    None = 0,

    /// Attachment signatures.
    Attachments = 1,
}

impl From<bool> for PackageSignaturesMode {
    fn from(value: bool) -> Self {
        if value { Self::Attachments } else { Self::None }
    }
}

/// Package in a send email request.
#[serde_as]
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Package {
    /// The per address sub-packages.
    pub addresses: PackageAddresses,

    /// The mime type of the package body.
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "MIMEType")]
    pub mime_type: PackageMimeType,

    /// The package type derived from the `address_type` sub-packages
    #[serde(rename = "Type")]
    pub package_type: u8,

    /// An exposed body session key to decrypt the body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_key: Option<ExposedKey>,

    /// An the exposed attachment keys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_keys: Option<PackageAttachmentEntries<ExposedKey>>,

    /// The raw body.
    ///
    /// TODO: In forms this could be bytes instead of b64
    #[serde_as(as = "Option<Base64>")]
    pub body: Option<Vec<u8>>,
}

/// Per address sub-package in a send email request.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddressSubPackage {
    /// The encryption type to this address.
    #[serde(rename = "Type")]
    pub address_type: PackageCryptoType,

    /// The encrypted body session key towards the address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_key_packet: Option<KeyPacket>,

    /// The encrypted attachment session keys towards the address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_key_packets: Option<PackageAttachmentEntries<KeyPacket>>,

    /// The encrypted  attachment signatures towards the address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_enc_signatures:
        Option<PackageAttachmentEntries<Base64AttachmentEncryptedSignature>>,

    /// The signature mode towards this address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignaturesMode>,

    /// TODO: add docs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// TODO: add docs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enc_token: Option<String>,

    /// TODO: add docs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthInput>,

    /// TODO: add docs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hint: Option<String>,
}

/// Represents authentication input for EO.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInput {
    /// The version of the authentication.
    pub version: u8,

    /// The modulus ID for authentication.
    #[serde(rename = "ModulusID")]
    pub modulus_id: String,

    /// The salt used in authentication.
    pub salt: String,

    /// The verifier for authentication.
    pub verifier: String,
}

/// A session key that is exposed to the backend.
#[derive(Debug, PartialEq, Serialize, Eq, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ExposedKey {
    /// The exposed session key base64 encoded.
    pub key: SessionKeyExposed,

    /// The session key algorithm of the exposed key.
    pub algorithm: SessionKeyAlgorithm,
}

impl From<InboxSessionKey> for ExposedKey {
    fn from(value: InboxSessionKey) -> Self {
        Self {
            key: value.expose_secret(),
            algorithm: value.algorithm(),
        }
    }
}

/// Defines newly created attachment disposition.
#[derive(Debug)]
pub enum NewAttachmentDisposition {
    /// Regular mail attachment.
    Attachment,
    /// Inline attachment, requires a content id.
    Inline(String),
}

/// Parameters required to create a new attachment.
#[derive(Debug)]
pub struct NewAttachmentParams {
    /// File name of the attachment.
    pub filename: String,
    /// Message to which this attachment belongs to.
    pub message_id: MessageId,
    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    pub mime_type: String,
    /// Attachment disposition.
    pub disposition: NewAttachmentDisposition,
    /// Binary asymmetric key packet.
    pub key_packets: Vec<u8>,
    /// Optional armored detached signature
    pub signature: Option<BinaryAttachmentSignature>,
    /// Optional armored encrypted message containing binary detached signature.
    pub enc_signature: Option<BinaryAttachmentEncryptedSignature>,
    /// Encrypted attachment payload.
    pub data_packet: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PutMobileSettings {
    pub conversation_toolbar: Vec<MobileAction>,

    pub list_toolbar: Vec<MobileAction>,

    pub message_toolbar: Vec<MobileAction>,
}

// TODO rename DraftSender into Sender etc.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectParams {
    pub subject: String,
    pub sender: DraftSender,
    pub to_list: Vec<DraftRecipient>,
    pub body: EncryptedDraft,
    pub attachments: Vec<DirectAttachment>,
}

#[serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectAttachment {
    pub filename: String,
    #[serde(rename = "MIMEType")]
    pub mimetype: String,
    #[serde_as(as = "Base64")]
    pub contents: Vec<u8>,
}

impl DirectAttachment {
    pub const INVITE_ICS: &str = "invite.ics";

    #[must_use]
    pub fn new(filename: &str, mimetype: &str, contents: &EncryptedAttachment) -> Self {
        let contents = {
            let mut payload = Vec::new();

            payload.extend(&contents.data);
            payload.extend(&contents.metadata.key_packets);

            if let Some(sign) = &contents.metadata.signature {
                payload.extend(sign.as_slice());
            }

            payload
        };

        Self {
            filename: filename.into(),
            mimetype: mimetype.into(),
            contents,
        }
    }

    #[must_use]
    pub fn invite_reply(ics: &EncryptedAttachment) -> Self {
        Self::new(Self::INVITE_ICS, "text/calendar; method=reply", ics)
    }
}
