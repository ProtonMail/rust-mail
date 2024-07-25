//! Data types for Proton Mail.
//!
//! This module contains the various data types used by Proton Mail, i.e. those
//! that are specific to the Proton Mail application. They are used in addition
//! to those presented from the Proton Core library.
//!
//! # Organisation
//!
//! The vast majority of the available data types are presented through this
//! module, and the focus is on those data types that are persistent, i.e.
//! stored in the database. In some cases there are special types with a
//! specific purpose that might be presented elsewhere. This method of
//! organisation may change over time as better patterns evolve.
//!
//! # Rust internals
//!
//! The types exposed here are carefully-prepared, lightweight facades that are
//! somewhat but not exactly analogous to the internal types used by the Proton
//! Core library. They are designed to be used by the FFI bindings, and are
//! prepared with those in mind. In this way they represent a translation layer
//! between the internal types and the FFI types, in the same way that there is
//! also a translation layer between the internal types and the Proton REST API
//! types. This gives the full ability to amend the external FFI interface as
//! necessary without affecting the internal types, and vice versa.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton internal
//! types to the exported FFI types and vice versa are provided, but not any
//! serialisation or deserialisation or other conversions. The conversions to
//! and from internal types are usually very simple and indeed in many cases can
//! be done without altering any data in memory.
//!
//! This separation does cause some duplication, but the overlap is not total.
//! The various implementations for the types differ in each place; any logic
//! for the application is in the internal types and not the FFI types; and
//! the distinction allows customisation of how the FFI types work.
//!

use proton_core_common::datatypes::{LabelId, RemoteId};
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum AlmostAllMail {
    /// TODO: Document this variant.
    AllMail = 0,

    /// TODO: Document this variant.
    #[default]
    AlmostAllMail = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ComposerDirection {
    /// TODO: Document this variant.
    #[default]
    LeftToRight = 0,

    /// TODO: Document this variant.
    RightToLeft = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ComposerMode {
    /// TODO: Document this variant.
    #[default]
    Normal = 0,

    /// TODO: Document this variant.
    Maximized = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum Disposition {
    /// TODO: Document this variant.
    Attachment = 1,

    /// TODO: Document this variant.
    Inline = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum LabelType {
    /// TODO: Document this field.
    Label = 1,

    /// TODO: Document this field.
    ContactGroup = 2,

    /// TODO: Document this field.
    Folder = 3,

    /// TODO: Document this field.
    System = 4,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum MessageButtons {
    /// TODO: Document this variant.
    #[default]
    ReadFirst = 0,

    /// TODO: Document this variant.
    UnreadFirst = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum MimeType {
    /// TODO: Document this variant.
    ApplicationJson = 1,

    /// TODO: Document this variant.
    ApplicationPdf = 2,

    /// TODO: Document this variant.
    MessageRfc822 = 3,

    /// TODO: Document this variant.
    MultipartMixed = 4,

    /// TODO: Document this variant.
    MultipartRelated = 5,

    /// TODO: Document this variant.
    #[default]
    TextHtml = 6,

    /// TODO: Document this variant.
    TextPlain = 7,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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

/// A message parsed header value can either be a string or an array of strings.
#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum ParsedHeaderValue {
    /// TODO: Document this variant.
    Array(Vec<String>),

    /// TODO: Document this variant.
    String(String),
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum PgpScheme {
    /// TODO: Document this variant.
    Inline = 8,

    /// TODO: Document this variant.
    #[default]
    Mime = 16,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum SpamAction {
    /// TODO: Document this variant.
    DoNothing = 0,

    /// TODO: Document this variant.
    UnsubscribeWithOneClick = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ViewLayout {
    /// TODO: Document this variant.
    #[default]
    Column = 0,

    /// TODO: Document this variant.
    Row = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Attachment {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub remote_address_id: RemoteId,

    /// TODO: Document this field.
    pub local_conversation_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_conversation_id: RemoteId,

    /// TODO: Document this field.
    pub local_message_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_message_id: RemoteId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub is_auto_forwardee: bool,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub real_enc_signature: Option<RealAttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub real_key_packets: Option<RealKeyPackets>,

    /// TODO: Document this field.
    pub real_signature: Option<RealAttachmentSignature>,

    /// TODO: Document this field.
    pub sender: Option<MessageAddress>,

    /// TODO: Document this field.
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
    pub size: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentEncryptedSignature {
    pub value: String,
}

impl Deref for AttachmentEncryptedSignature {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentMetadata {
    /// TODO: Document this field.
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentMetadatas {
    pub value: Vec<AttachmentMetadata>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentSignature {
    pub value: String,
}

impl Deref for AttachmentSignature {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Conversation {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub attachment_info: MessageAttachmentInfos,

    /// TODO: Document this field.
    pub attachments_metadata: AttachmentMetadatas,

    /// TODO: Document this field.
    pub deleted: bool,

    /// TODO: Document this field.
    pub display_snooze_reminder: bool,

    /// TODO: Document this field.
    pub expiration_time: u64,

    /// TODO: Document this field.
    pub num_attachments: u64,

    /// TODO: Document this field.
    pub num_messages: u64,

    /// TODO: Document this field.
    pub num_unread: u64,

    /// TODO: Document this field.
    pub display_order: u64,

    /// TODO: Document this field.
    pub recipients: MessageAddresses,

    /// TODO: Document this field.
    pub senders: MessageAddresses,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    pub subject: String,
}

/// TODO: Document this struct.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ConversationCount {
    /// TODO: Document this field.
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ConversationLabel {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    pub local_conversation_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_conversation_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub local_label_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_label_id: Option<LabelId>,

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

/// Consists of the message's body metadata and decrypted content.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct DecryptedMessageBody {
    /// The decrypted message contents.
    pub body: String,

    /// Metadata associated with the message body
    pub metadata: MessageBodyMetadata,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct EncryptedMessageBody {
    /// TODO: Document this field.
    pub encrypted_body: String,

    /// TODO: Document this field.
    pub metadata: MessageBodyMetadata,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct KeyPackets {
    pub value: String,
}

impl Deref for KeyPackets {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct Label {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<LabelId>,

    /// TODO: Document this field.
    pub local_parent_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_parent_id: Option<LabelId>,

    /// TODO: Document this field.
    pub color: LabelColor,

    /// TODO: Document this field.
    pub display: bool,

    /// TODO: Document this field.
    pub expanded: bool,

    /// TODO: Document this field.
    pub initialized_conv: bool,

    /// TODO: Document this field.
    pub initialized_msg: bool,

    /// TODO: Document this field.
    pub label_type: LabelType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub notify: bool,

    /// TODO: Document this field.
    pub display_order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    pub sticky: bool,

    /// TODO: Document this field.
    pub total_conv: u64,

    /// TODO: Document this field.
    pub total_msg: u64,

    /// TODO: Document this field.
    pub unread_conv: u64,

    /// TODO: Document this field.
    pub unread_msg: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct LabelColor {
    value: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, SmartDefault, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct MailSettings {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    pub almost_all_mail: AlmostAllMail,

    /// TODO: Document this field.
    pub attach_public_key: bool,

    /// TODO: Document this field.
    pub auto_delete_spam_and_trash_days: Option<u32>,

    /// TODO: Document this field.
    #[default = true]
    pub auto_save_contacts: bool,

    /// TODO: Document this field.
    pub block_sender_confirmation: Option<bool>,

    /// TODO: Document this field.
    pub composer_mode: ComposerMode,

    /// TODO: Document this field.
    #[default = true]
    pub confirm_link: bool,

    /// TODO: Document this field.
    #[default = 10]
    pub delay_send_seconds: u32,

    /// TODO: Document this field.
    pub display_name: String,

    /// TODO: Document this field.
    pub draft_mime_type: MimeType,

    /// TODO: Document this field.
    pub enable_folder_color: bool,

    /// TODO: Document this field.
    pub font_face: Option<String>,

    /// TODO: Document this field.
    pub hide_remote_images: bool,

    /// TODO: Document this field.
    pub hide_sender_images: bool,

    /// TODO: Document this field.
    pub image_proxy: u32,

    /// TODO: Document this field.
    #[default = true]
    pub inherit_parent_folder_color: bool,

    /// TODO: Document this field.
    pub message_buttons: MessageButtons,

    /// TODO: Document this field.
    pub mobile_settings: Option<MobileSettings>,

    /// TODO: Document this field.
    pub next_message_on_move: Option<NextMessageOnMove>,

    /// TODO: Document this field.
    pub num_message_per_page: u32,

    /// TODO: Document this field.
    pub pgp_scheme: PgpScheme,

    /// TODO: Document this field.
    pub pm_signature: PmSignature,

    /// TODO: Document this field.
    #[default = true]
    pub pm_signature_referral_link: bool,

    /// TODO: Document this field.
    pub prompt_pin: bool,

    /// TODO: Document this field.
    pub receive_mime_type: MimeType,

    /// TODO: Document this field.
    pub right_to_left: ComposerDirection,

    /// TODO: Document this field.
    #[default = true]
    pub shortcuts: bool,

    /// TODO: Document this field.
    pub show_images: ShowImages,

    /// TODO: Document this field.
    pub show_mime_type: MimeType,

    /// TODO: Document this field.
    pub show_moved: ShowMoved,

    /// TODO: Document this field.
    pub sign: bool,

    /// TODO: Document this field.
    pub signature: String,

    /// TODO: Document this field.
    pub spam_action: Option<SpamAction>,

    /// TODO: Document this field.
    pub sticky_labels: bool,

    /// TODO: Document this field.
    pub submission_access: bool,

    /// TODO: Document this field.
    pub swipe_left: SwipeAction,

    /// TODO: Document this field.
    pub swipe_right: SwipeAction,

    /// TODO: Document this field.
    pub theme: String,

    /// TODO: Document this field.
    pub view_layout: ViewLayout,

    /// TODO: Document this field.
    pub view_mode: ViewMode,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct Message {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub local_conversation_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_conversation_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub address_id: RemoteId,

    /// TODO: Document this field.
    pub attachments: MessageAttachments,

    /// TODO: Document this field.
    pub attachments_metadata: AttachmentMetadatas,

    /// TODO: Document this field.
    pub bcc_list: MessageAddresses,

    /// TODO: Document this field.
    pub cc_list: MessageAddresses,

    /// TODO: Document this field.
    pub deleted: bool,

    /// TODO: Document this field.
    pub expiration_time: u64,

    /// TODO: Document this field.
    pub external_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub header: String,

    /// TODO: Document this field.
    pub flags: MessageFlags,

    /// TODO: Document this field.
    pub is_forwarded: bool,

    /// TODO: Document this field.
    pub is_replied: bool,

    /// TODO: Document this field.
    pub is_replied_all: bool,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub num_attachments: u32,

    /// TODO: Document this field.
    pub display_order: u64,

    /// TODO: Document this field.
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    pub parsed_headers: ParsedHeaders,

    /// TODO: Document this field.
    pub reply_tos: MessageAddresses,

    /// TODO: Document this field.
    pub sender: MessageAddress,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    pub snooze_time: u64,

    /// TODO: Document this field.
    pub subject: String,

    /// TODO: Document this field.
    pub time: u64,

    /// TODO: Document this field.
    pub to_list: MessageAddresses,

    /// TODO: Document this field.
    pub unread: bool,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageAddress {
    /// TODO: Document this field.
    // TODO: Proper email parsing
    pub address: String,

    /// TODO: Document this field.
    pub bimi_selector: Option<String>,

    /// TODO: Document this field.
    pub display_sender_image: bool,

    /// TODO: Document this field.
    pub is_proton: bool,

    /// TODO: Document this field.
    pub is_simple_login: bool,

    /// TODO: Document this field.
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct MessageAddresses {
    pub value: Vec<MessageAddress>,
}

#[derive(Debug, Clone, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachment {
    /// TODO: Document this field.
    pub id: RemoteId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub headers: MessageAttachmentHeaders,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachmentHeaders {
    /// TODO: Document this field.
    pub content_disposition: String,

    /// TODO: Document this field.
    pub content_id: Option<String>,

    /// TODO: Document this field.
    pub content_transfer_encoding: Option<String>,

    /// TODO: Document this field.
    pub image_height: Option<String>,

    /// TODO: Document this field.
    pub image_width: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachmentInfo {
    /// TODO: Document this field.
    pub attachment: u32,

    /// TODO: Document this field.
    pub inline: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachmentInfos {
    pub value: HashMap<String, MessageAttachmentInfo>,
}

impl Deref for MessageAttachmentInfos {
    type Target = HashMap<String, MessageAttachmentInfo>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for MessageAttachmentInfos {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachments {
    pub value: Vec<MessageAttachment>,
}

impl Deref for MessageAttachments {
    type Target = Vec<MessageAttachment>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for MessageAttachments {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Metadata associated with the Body of a message.
///
/// Message bodies are not stored in the database.
///
/// Note that this information does not come directly from the API, and so there
/// is no equivalent API struct to convert from. Rather, the metadata is
/// obtained from [`DecryptedMessageBody`].
///
/// For metadata associated with a message see [`MessageMetadata`].
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageBodyMetadata {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub local_message_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_message_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub header: String,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub parsed_headers: ParsedHeaders,
}

/// TODO: Document this struct.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageCount {
    /// TODO: Document this field.
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageFlags {
    value: u64,
}

impl From<MessageFlags> for RealMessageFlags {
    fn from(value: MessageFlags) -> Self {
        RealMessageFlags::from_bits_truncate(value.0)
    }
}

impl From<RealMessageFlags> for MessageFlags {
    fn from(value: RealMessageFlags) -> Self {
        MessageFlags(value.bits())
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MobileSetting {
    /// TODO: Document this field.
    pub actions: Vec<String>,

    /// TODO: Document this field.
    pub is_custom: bool,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MobileSettings {
    /// TODO: Document this field.
    pub conversation_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub list_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub message_toolbar: MobileSetting,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct ParsedHeaders {
    pub headers: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct RemoteIds {
    pub value: Vec<RemoteId>,
}
