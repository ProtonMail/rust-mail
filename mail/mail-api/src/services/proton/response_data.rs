//! Response child data structures for the Proton Mail API.
//!
//! This module provides child data types that are used by the response
//! structures when receiving requests from the Proton Mail API.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint response
//! definitions, and NOT have any business logic or other functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for incoming data.
//!
//! Structs in this module should only implement [`Deserialize`], and should not
//! implement [`Serialize`](serde::Serialize). If anything in this module
//! implements [`Serialize`](serde::Serialize), it is a sign that a mistake has
//! been made. The exception here is for testing purposes, e.g. when mocking
//! response data — in which case implementing [`Serialize`](serde::Serialize)
//! conditionally, only in test mode, is advised.
//!
//! Any types that used by both requests and responses should be defined in the
//! [`common`](crate::services::proton::common) module.
//!

use crate::services::proton::common::LabelType;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::services::proton::response_data::{
    Action, Address, ApiErrorInfo, ContactEmailEvent, ContactEvent, ProductUsedSpace, User,
    UserSettings,
};
use proton_api_core::services::proton::responses::GetEventResponse;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use serde::Deserialize;
use serde::Serialize;
use serde_repr::Deserialize_repr;
#[cfg(any(test, debug_assertions))]
use serde_repr::Serialize_repr;
use serde_with::{serde_as, BoolFromInt, DefaultOnNull};
use smart_default::SmartDefault;
use std::collections::{BTreeMap, HashMap};

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum AlmostAllMail {
    /// TODO: Document this variant.
    AllMail = 0,

    /// TODO: Document this variant.
    #[default]
    AlmostAllMail = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ComposerDirection {
    /// TODO: Document this variant.
    #[default]
    LeftToRight = 0,

    /// TODO: Document this variant.
    RightToLeft = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ComposerMode {
    /// TODO: Document this variant.
    #[default]
    Normal = 0,

    /// TODO: Document this variant.
    Maximized = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// TODO: Document this variant.
    Attachment,

    /// TODO: Document this variant.
    Inline,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum MessageButtons {
    /// TODO: Document this variant.
    #[default]
    ReadFirst = 0,

    /// TODO: Document this variant.
    UnreadFirst = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum MimeType {
    /// TODO: Document this variant.
    #[serde(rename = "application/json")]
    ApplicationJson,

    /// TODO: Document this variant.
    #[serde(rename = "application/pdf")]
    ApplicationPdf,

    /// TODO: Document this variant.
    #[serde(rename = "message/rfc822")]
    MessageRfc822,

    /// TODO: Document this variant.
    #[serde(rename = "multipart/mixed")]
    MultipartMixed,

    /// TODO: Document this variant.
    #[serde(rename = "multipart/related")]
    MultipartRelated,

    /// TODO: Document this variant.
    #[default]
    #[serde(rename = "text/html")]
    TextHtml,

    /// TODO: Document this variant.
    #[serde(rename = "text/plain")]
    TextPlain,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum NextMessageOnMove {
    /// TODO: Document this variant.
    #[default]
    DisabledExplicit = 0,

    /// TODO: Document this variant.
    DisabledImplicit = 1,

    /// TODO: Document this variant.
    EnabledExplicit = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum PgpScheme {
    /// TODO: Document this variant.
    Inline = 8,

    /// TODO: Document this variant.
    #[default]
    Mime = 16,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum PmSignature {
    /// TODO: Document this variant.
    #[default]
    Disabled = 0,

    /// TODO: Document this variant.
    Enabled = 1,

    /// TODO: Document this variant.
    EnabledLocked = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ShowImages {
    /// TODO: Document this variant.
    DoNotAutoLoad = 0,

    /// TODO: Document this variant.
    AutoLoadRemote = 1,

    /// TODO: Document this variant.
    #[default]
    AutoLoadEmbedded = 2,

    /// TODO: Document this variant.
    AutoLoadBoth = 3,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ShowMoved {
    /// TODO: Document this variant.
    #[default]
    DoNotKeep = 0,

    /// TODO: Document this variant.
    KeepInDrafts = 1,

    /// TODO: Document this variant.
    KeepInSent = 2,

    /// TODO: Document this variant.
    KeepBoth = 3,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum SpamAction {
    /// TODO: Document this variant.
    DoNothing = 0,

    /// TODO: Document this variant.
    UnsubscribeWithOneClick = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum SwipeAction {
    /// TODO: Document this variant.
    Trash = 0,

    /// TODO: Document this variant.
    Spam = 1,

    /// TODO: Document this variant.
    Star = 2,

    /// TODO: Document this variant.
    #[default]
    Archive = 3,

    /// TODO: Document this variant.
    MarkAsRead = 4,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ViewLayout {
    /// TODO: Document this variant.
    #[default]
    Column = 0,

    /// TODO: Document this variant.
    Row = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum ViewMode {
    /// TODO: Document this variant.
    #[default]
    Conversations = 0,

    /// TODO: Document this variant.
    Messages = 1,
}

//  STRUCTS
//==============================================================================

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Attachment {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "AddressID")]
    pub address_id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "ConversationID")]
    pub conversation_id: RemoteId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub is_auto_forwardee: bool,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// TODO: Document this field.
    #[serde(rename = "MessageID")]
    pub message_id: RemoteId,

    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    #[serde(rename = "MIMEType")]
    pub mime_type: String,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub sender: Option<MessageAddress>,

    /// TODO: Document this field.
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
    pub size: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct AttachmentMetadata {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    #[serde(rename = "MIMEType")]
    pub mime_type: String,

    /// TODO: Document this field.
    #[serde(default)]
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct AutoResponder {
    /// TODO: Document this field.
    pub days_selected: Vec<String>,

    /// TODO: Document this field.
    pub end_time: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub is_enabled: bool,

    /// TODO: Document this field.
    pub message: String,

    /// TODO: Document this field.
    pub repeat: u64,

    /// TODO: Document this field.
    pub start_time: u64,

    /// TODO: Document this field.
    pub subject: String,

    /// TODO: Document this field.
    pub zone: String,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Conversation {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachment_info: BTreeMap<String, MessageAttachmentInfo>,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub display_snooze_reminder: bool,

    /// TODO: Document this field.
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub labels: Vec<ConversationLabel>,

    /// TODO: Document this field.
    pub num_attachments: u64,

    /// TODO: Document this field.
    pub num_messages: u64,

    /// TODO: Document this field.
    pub num_unread: u64,

    /// TODO: Document this field.
    pub order: u64,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnNull")]
    pub recipients: Vec<MessageAddress>,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnNull")]
    pub senders: Vec<MessageAddress>,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    pub subject: String,
}

#[cfg(any(test, debug_assertions))]
impl Default for Conversation {
    fn default() -> Self {
        Self {
            id: RemoteId::from(""),
            attachment_info: BTreeMap::default(),
            attachments_metadata: Vec::default(),
            display_snooze_reminder: false,
            expiration_time: 0,
            labels: Vec::default(),
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            order: 0,
            recipients: Vec::default(),
            senders: Vec::default(),
            size: 0,
            subject: String::default(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationCount {
    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// Data for an event related to a [`ConversationEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub conversation: Option<Conversation>,
}

impl GetEventResponse for ConversationEvent {}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationLabel {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub context_expiration_time: u64,

    /// TODO: Document this field.
    pub context_num_attachments: u64,

    /// TODO: Document this field.
    pub context_num_messages: u64,

    /// TODO: Document this field.
    pub context_num_unread: u64,

    /// TODO: Document this field.
    pub context_size: u64,

    /// TODO: Document this field.
    pub context_snooze_time: u64,

    /// TODO: Document this field.
    pub context_time: u64,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Label {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub display: bool,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub expanded: bool,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub notify: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub sticky: bool,
}

#[cfg(any(test, debug_assertions))]
impl Default for Label {
    fn default() -> Self {
        Self {
            id: RemoteId::from(""),
            parent_id: None,
            color: String::default(),
            display: false,
            expanded: false,
            label_type: LabelType::Label,
            name: String::default(),
            notify: false,
            order: 0,
            path: None,
            sticky: false,
        }
    }
}

/// Data for an event related to a [`LabelEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct LabelEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub label: Option<Label>,
}

impl GetEventResponse for LabelEvent {}

/// Data for an event related to a [`MailEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MailEvent {
    /// TODO: Document this field.
    #[serde(rename = "EventID")]
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub addresses: Option<Vec<Address>>,

    /// TODO: Document this field.
    pub conversation_counts: Option<Vec<ConversationCount>>,

    /// TODO: Document this field.
    pub conversations: Option<Vec<ConversationEvent>>,

    /// TODO: Document this field.
    #[serde(rename = "More")]
    #[serde_as(as = "BoolFromInt")]
    pub has_more: bool,

    /// TODO: Document this field.
    pub labels: Option<Vec<LabelEvent>>,

    /// TODO: Document this field.
    pub mail_settings: Option<MailSettings>,

    /// TODO: Document this field.
    pub message_counts: Option<Vec<MessageCount>>,

    /// TODO: Document this field.
    pub messages: Option<Vec<MessageEvent>>,

    /// TODO: Document this field.
    pub product_used_space: Option<ProductUsedSpace>,

    /// TODO: Document this field.
    pub used_space: Option<i64>,

    /// TODO: Document this field.
    pub user: Option<User>,

    /// TODO: Document this field.
    pub user_settings: Option<UserSettings>,

    /// TODO: Document this field.
    pub contacts: Option<Vec<ContactEvent>>,

    /// TODO: Document this field.
    pub contact_emails: Option<Vec<ContactEmailEvent>>,
}

impl GetEventResponse for MailEvent {}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, SmartDefault)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct MailSettings {
    /// TODO: Document this field.
    #[serde(default)]
    pub almost_all_mail: AlmostAllMail,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub attach_public_key: bool,

    /// TODO: Document this field.
    pub auto_delete_spam_and_trash_days: Option<u32>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    #[default = true]
    pub auto_save_contacts: bool,

    /// TODO: Document this field.
    #[serde_as(as = "Option<DefaultOnNull<BoolFromInt>>")]
    pub block_sender_confirmation: Option<bool>,

    /// TODO: Document this field.
    #[serde(default)]
    pub composer_mode: ComposerMode,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    #[default = true]
    pub confirm_link: bool,

    /// TODO: Document this field.
    #[default = 10]
    pub delay_send_seconds: u32,

    /// TODO: Document this field.
    pub display_name: String,

    /// TODO: Document this field.
    #[serde(rename = "DraftMIMEType")]
    pub draft_mime_type: MimeType,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub enable_folder_color: bool,

    /// TODO: Document this field.
    pub font_face: Option<String>,

    /// This enables or disables remote content in the HTML.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub hide_remote_images: bool,

    /// This enables or disables embedded content (`Disposition::Inline`) in the HTML.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub hide_embedded_images: bool,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub hide_sender_images: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub image_proxy: u32,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    #[default = true]
    pub inherit_parent_folder_color: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub message_buttons: MessageButtons,

    /// TODO: Document this field.
    pub mobile_settings: Option<MobileSettings>,

    /// TODO: Document this field.
    pub next_message_on_move: Option<NextMessageOnMove>,

    /// TODO: Document this field.
    pub num_message_per_page: u32,

    /// TODO: Document this field.
    #[serde(default, rename = "PGPScheme")]
    pub pgp_scheme: PgpScheme,

    /// TODO: Document this field.
    #[serde(rename = "PMSignature", default)]
    pub pm_signature: PmSignature,

    /// TODO: Document this field.
    #[serde(rename = "PMSignatureReferralLink")]
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    #[default = true]
    pub pm_signature_referral_link: bool,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub prompt_pin: bool,

    /// TODO: Document this field.
    #[serde(rename = "ReceiveMIMEType")]
    pub receive_mime_type: MimeType,

    /// TODO: Document this field.
    #[serde(default)]
    pub right_to_left: ComposerDirection,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    #[default = true]
    pub shortcuts: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub show_images: ShowImages,

    /// TODO: Document this field.
    #[serde(rename = "ShowMIMEType")]
    pub show_mime_type: MimeType,

    /// TODO: Document this field.
    #[serde(default)]
    pub show_moved: ShowMoved,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub sign: bool,

    /// TODO: Document this field.
    pub signature: String,

    /// TODO: Document this field.
    pub spam_action: Option<SpamAction>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub sticky_labels: bool,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub submission_access: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub swipe_left: SwipeAction,

    /// TODO: Document this field.
    #[serde(default)]
    pub swipe_right: SwipeAction,

    /// TODO: Document this field.
    pub theme: String,

    /// TODO: Document this field.
    #[serde(default)]
    pub view_layout: ViewLayout,

    /// TODO: Document this field.
    #[serde(default)]
    pub view_mode: ViewMode,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Message {
    /// TODO: Document this field.
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,

    /// TODO: Document this field.
    pub body: String,

    /// TODO: Document this field.
    pub header: String,

    /// TODO: Document this field.
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,

    /// TODO: Document this field.
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    pub parsed_headers: HashMap<String, serde_json::Value>,

    /// TODO: Document this field.
    #[serde(flatten)]
    pub metadata: MessageMetadata,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAttachment {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub headers: MessageAttachmentHeaders,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    #[serde(rename = "MIMEType")]
    pub mime_type: String,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
    pub size: u64,
}

#[cfg(any(test, debug_assertions))]
impl Default for Message {
    fn default() -> Self {
        Self {
            attachments: Vec::default(),
            body: String::default(),
            header: String::default(),
            mime_type: MimeType::TextPlain,
            parsed_headers: HashMap::default(),
            metadata: MessageMetadata::default(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAttachmentInfo {
    /// TODO: Document this field.
    #[serde(default)]
    pub attachment: u32,

    #[serde(default)]
    pub inline: u32,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAttachmentHeaders {
    /// TODO: Document this field.
    #[serde(rename = "content-disposition")]
    pub content_disposition: String,

    #[serde(rename = "content-id")]
    pub content_id: Option<String>,

    #[serde(rename = "content-transfer-encoding")]
    pub content_transfer_encoding: Option<String>,

    #[serde(rename = "x-pm-image-height")]
    pub image_height: Option<String>,

    #[serde(rename = "x-pm-image-width")]
    pub image_width: Option<String>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageCount {
    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: RemoteId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// Data for an event related to a [`MessageEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub message: Option<MessageMetadata>,
}

impl GetEventResponse for MessageEvent {}

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(transparent)]
#[repr(transparent)]
pub struct MessageFlags(u64);

bitflags::bitflags! {
    impl MessageFlags:u64 {
        /// Whether a message has been received.
        const RECEIVED = 1;

        /// Whether a message has been sent.
        const SENT = 1 << 1;

        /// Whether the message is between Proton Mail recipients.
        const INTERNAL = 1 << 2;

        /// Whether the message is end-to-end encrypted.
        const E2E = 1 << 3;

        /// Whether the message is an auto response.
        const AUTO = 1 << 4;

        /// Whether the message has been replied to.
        const REPLIED = 1 << 5;

        /// Whether the message was replied to using reply to all.
        const REPLIED_ALL = 1 << 6;

        /// Whether the message has been forwarded.
        const FORWARDED = 1 << 7;

        /// Whether the message has been responded to with an auto response.
        const AUTO_REPLIED = 1 << 8;

        /// Whether the message is an import.
        const IMPORTED = 1 << 9;

        /// Whether the message has ever been opened by the user.
        const OPENED = 1 << 10;

        /// Whether a read receipt has been sent in response to the message.
        const RECEIPT_SENT = 1 << 11;

        /// No longer used.
        const UNUSED_1 = 1 << 12;

        /// No longer used.
        const UNUSED_2 = 1 << 13;

        /// Whether the message is a receipt.
        const RECEIPT = 1 << 14;

        /// Whether the message is from Proton.
        const PROTON = 1 << 15;

        /// Whether to request a read receipt for the message.
        const RECEIPT_REQUEST = 1 << 16;

        /// Whether to attach a public key.
        const PUBLIC_KEY = 1 << 17;

        /// Whether to sign the message.
        const SIGN = 1 << 18;

        /// Unsubscribed from newsletter.
        const UNSUBSCRIBED = 1 << 19;

        /// Messages that have been scheduled to send at a later time.
        const SCHEDULED_SEND = 1 << 20;

        /// No longer used.
        const UNUSED_3 = 1 << 21;

        /// Whether the message was synced from Gmail.
        const SYNCED_FROM_GMAIL = 1 << 22;

        /// Whether DMARC authentication passed.
        const DMARC_PASS = 1 << 23;

        /// Whether the message failed an SPF check.
        const SPF_FAIL = 1 << 24;

        /// Whether then message failed a DKIM check.
        const DKIM_FAIL = 1 << 25;

        /// Whether the incoming message failed DMARC authentication.
        const DMARC_FAIL = 1 << 26;

        /// Whether the message is in spam and the user moves it to a new
        /// location that is not spam or trash (e.g. inbox or archive).
        const HAM_MANUAL = 1 << 27;

        /// Whether the message has been marked as spam by anti-spam filters.
        const SPAM_AUTO = 1 << 28;

        /// Whether the message has been manually marked as spam.
        const SPAM_MANUAL = 1 << 29;

        /// Whether the message has been marked as phishing by anti-spam filters.
        const PHISHING_AUTO = 1 << 30;

        /// Whether the message has been manually marked as phishing.
        const PHISHING_MANUAL = 1 << 31;

        /// Messages where the expiration time cannot be changed.
        const FROZEN_EXPIRATION = 1 << 32;

        /// Whether the message has been flagged as suspicious by the system.
        const FLAG_SUSPICIOUS = 1 << 33;

        /// Whether the message has been auto-forwarded.
        const FLAG_AUTO_FORWARDER = 1 << 34;

        /// Whether the message has been auto-forwarded.
        const FLAG_AUTO_FORWARDEE = 1 << 35;
    }
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct MessageMetadata {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "ConversationID")]
    pub conversation_id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "AddressID")]
    pub address_id: RemoteId,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[serde(rename = "BCCList", default)]
    pub bcc_list: Vec<MessageAddress>,

    /// TODO: Document this field.
    #[serde(rename = "CCList", default)]
    pub cc_list: Vec<MessageAddress>,

    /// TODO: Document this field.
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[serde(rename = "ExternalID")]
    pub external_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub flags: MessageFlags,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub is_forwarded: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub is_replied: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub is_replied_all: bool,

    /// TODO: Document this field.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<RemoteId>,

    /// TODO: Document this field.
    pub num_attachments: u32,

    /// TODO: Document this field.
    pub order: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub reply_tos: Vec<MessageAddress>,

    /// TODO: Document this field.
    #[serde(default)]
    pub sender: MessageAddress,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub snooze_time: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub subject: String,

    /// TODO: Document this field.
    pub time: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub to_list: Vec<MessageAddress>,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub unread: bool,
}

#[cfg(any(test, debug_assertions))]
impl Default for MessageMetadata {
    fn default() -> Self {
        Self {
            id: RemoteId::from(""),
            conversation_id: RemoteId::from(""),
            address_id: RemoteId::from(""),
            attachments_metadata: Vec::default(),
            bcc_list: Vec::default(),
            cc_list: Vec::default(),
            expiration_time: 0,
            external_id: None,
            flags: MessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: Vec::default(),
            num_attachments: 0,
            order: 0,
            reply_tos: Vec::default(),
            sender: MessageAddress::default(),
            size: 0,
            snooze_time: 0,
            subject: String::default(),
            time: 0,
            to_list: Vec::default(),
            unread: false,
        }
    }
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAddress {
    /// TODO: Document this field.
    // TODO: Proper email parsing
    pub address: String,

    /// TODO: Document this field.
    pub bimi_selector: Option<String>,

    /// TODO: Document this field
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub display_sender_image: bool,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub is_proton: bool,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub is_simple_login: bool,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MobileSetting {
    /// TODO: Document this field.
    #[serde(default)]
    pub actions: Vec<String>,

    /// TODO: Document this field.
    pub is_custom: bool,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MobileSettings {
    /// TODO: Document this field.
    pub conversation_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub list_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub message_toolbar: MobileSetting,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct OperationResult {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "Response")]
    pub response: ApiErrorInfo,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct UndoToken {
    /// TODO: Document this field.
    pub token: String,

    /// TODO: Document this field.
    #[serde(rename = "ValidUntil")]
    pub valid_until: u64,
}
