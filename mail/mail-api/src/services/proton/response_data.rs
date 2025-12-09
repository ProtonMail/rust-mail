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

use crate::services::proton::common::{AttachmentId, ConversationId, ExternalId, MessageId};
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{
    Action, CoreEvent, EventId, LabelEvent, PrivateEmail, PrivateString,
};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use serde::Deserialize;
use serde::Serialize;
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{BoolFromInt, DefaultOnNull, serde_as};
use smart_default::SmartDefault;
use std::collections::{BTreeMap, HashMap};

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum ComposerMode {
    /// TODO: Document this variant.
    #[default]
    Normal = 0,

    /// TODO: Document this variant.
    Maximized = 1,
}

/// Whether this is an embedded attachment.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// TODO: Document this variant.
    Attachment,
    Inline,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum PgpScheme {
    /// TODO: Document this variant.
    Inline = 8,

    /// TODO: Document this variant.
    #[default]
    Mime = 16,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[repr(transparent)]
pub struct PmSignature(pub u8);

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum SpamAction {
    /// TODO: Document this variant.
    DoNothing = 0,

    /// TODO: Document this variant.
    UnsubscribeWithOneClick = 1,
}

/// Where to move or what to do with the item when the user swipes it.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(i8)]
pub enum SwipeAction {
    NoAction = -1,

    Trash = 0,

    Spam = 1,

    Star = 2,

    #[default]
    Archive = 3,

    MarkAsRead = 4,

    LabelAs = 5,

    MoveTo = 6,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Attachment {
    /// The remote id of this attachment.
    #[serde(rename = "ID")]
    pub id: AttachmentId,

    /// TODO: Document this field.
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,

    /// What conversation this attachment belongs to
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,

    /// What message this attachment belongs to
    #[serde(rename = "MessageID")]
    pub message_id: MessageId,

    /// Whether this is an embedded attachment.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub is_auto_forwardee: bool,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// Attachment's MIME type may differ from the MIME type of the message.
    /// There is a lot of possible MIME types, so it is not possible to list
    /// all here. The safest bet is to deserialize it to string at that point.
    #[serde(rename = "MIMEType")]
    pub mime_type: String,

    /// The name of the attachment.
    pub name: String,

    /// TODO: Document this field.
    pub sender: Option<MessageSender>,

    /// See [`AttachmentSignature`]
    pub signature: Option<AttachmentSignature>,

    #[serde(default, rename = "ContentID")]
    pub content_id: Option<String>,

    /// The size of the attachment in bytes.
    pub size: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct AttachmentMetadata {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: AttachmentId,

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
#[cfg_attr(feature = "mocks", derive(Serialize))]
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
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Conversation {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ConversationId,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachment_info: BTreeMap<String, MessageAttachmentInfo>,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// Whether a snooze reminder should be displayed.
    /// It is set to true when the conversation is to be reminded.
    pub display_snoozed_reminder: bool,

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
    pub recipients: Vec<MessageRecipient>,

    /// TODO: Document this field.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnNull")]
    pub senders: Vec<MessageSender>,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    pub subject: String,

    /// Contextual dependent time for the conversation.
    ///
    /// Note: This should not be stored as it is not stable.
    /// Note: Field is not supported in `core/_v5/events` endpoint.
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnNull")]
    #[serde(skip_serializing)]
    pub context_time: Option<u64>,
}

#[cfg(feature = "mocks")]
impl Conversation {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            id: ConversationId::from(""),
            attachment_info: BTreeMap::default(),
            attachments_metadata: Vec::default(),
            display_snoozed_reminder: false,
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
            context_time: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationCount {
    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// Data for an event related to a [`ConversationEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ConversationId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub conversation: Option<Conversation>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ConversationLabel {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: LabelId,

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

#[cfg(feature = "mocks")]
impl ConversationLabel {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            id: "".into(),
            context_num_messages: 0,
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }
    }
}

/// Data for an event related to a [`MailEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MailEvent {
    #[serde(rename = "EventID")]
    pub event_id: EventId,

    pub labels: Option<Vec<LabelEvent>>,

    pub conversation_counts: Option<Vec<ConversationCount>>,

    pub conversations: Option<Vec<ConversationEvent>>,

    pub incoming_defaults: Option<Vec<IncomingDefaultEvent>>,

    pub mail_settings: Option<MailSettings>,

    pub message_counts: Option<Vec<MessageCount>>,

    pub messages: Option<Vec<MessageEvent>>,

    /// Indicates whether to refresh.
    pub refresh: u8,

    /// Whether we need to request more events after this.
    #[serde(rename = "More")]
    #[serde_as(as = "BoolFromInt")]
    pub has_more: bool,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MailEventV5 {
    #[serde(flatten)]
    pub core: CoreEvent,
    pub labels: Option<Vec<LabelEvent>>,

    pub conversation_counts: Option<Vec<ConversationCount>>,

    pub conversations: Option<Vec<ConversationEvent>>,

    pub incoming_defaults: Option<Vec<IncomingDefaultEvent>>,

    pub mail_settings: Option<MailSettings>,

    pub message_counts: Option<Vec<MessageCount>>,

    pub messages: Option<Vec<MessageEvent>>,
}

impl From<MailEvent> for MailEventV5 {
    fn from(m: MailEvent) -> Self {
        Self {
            core: CoreEvent {
                event_id: m.event_id,
                addresses: None,
                labels: None,
                product_used_space: None,
                used_space: None,
                user: None,
                user_settings: None,
                contacts: None,
                refresh: m.refresh,
                has_more: m.has_more,
            },
            labels: m.labels,
            conversation_counts: m.conversation_counts,
            conversations: m.conversations,
            incoming_defaults: m.incoming_defaults,
            mail_settings: m.mail_settings,
            message_counts: m.message_counts,
            messages: m.messages,
        }
    }
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, SmartDefault)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
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

/// Represents a message with its metadata and its body.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Message {
    #[serde(flatten)]
    pub metadata: MessageMetadata,

    #[serde(flatten)]
    pub body: MessageBody,
}

#[cfg(feature = "mocks")]
impl Message {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            metadata: MessageMetadata::test_default(),
            body: MessageBody::test_default(),
        }
    }
}

/// Contains metadata associated with the message body.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageBody {
    /// Attachment metadata associated with the message.
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,

    /// Encrypted message body
    pub body: String,

    pub reply_to: MessageReplyTo,

    pub reply_tos: Vec<MessageReplyTo>,

    /// Unparsed RFC822 message headers.
    pub header: String,

    /// Mime type of the body.
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,

    /// Parsed RFC822 message headers .
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    pub parsed_headers: HashMap<String, serde_json::Value>,
}

#[cfg(feature = "mocks")]
impl MessageBody {
    #[allow(clippy::default_trait_access)]
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            attachments: vec![],
            body: String::new(),
            reply_to: Default::default(),
            reply_tos: vec![],
            header: String::new(),
            mime_type: Default::default(),
            parsed_headers: Default::default(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAttachment {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: AttachmentId,

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

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
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
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageAttachmentHeaders {
    #[serde(rename = "content-disposition")]
    pub content_disposition: ContentDisposition,

    #[serde(rename = "content-id")]
    pub content_id: Option<String>,

    #[serde(rename = "content-transfer-encoding")]
    pub content_transfer_encoding: Option<String>,

    #[serde(rename = "x-pm-image-height")]
    pub image_height: Option<String>,

    #[serde(rename = "x-pm-image-width")]
    pub image_width: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentDisposition {
    One(String),
    Many(Vec<String>),
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageCount {
    /// TODO: Document this field.
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// Data for an event related to a [`MessageEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: MessageId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub message: Option<MessageMetadata>,
}

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[repr(transparent)]
pub struct MessageFlags(u64);

bitflags::bitflags! {
    impl MessageFlags:u64 {
        /// Whether a message has been received.
        const RECEIVED = 1 << 0;

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

        /// Message is a reply to an Encrypted-Outside message
        const FLAG_EO_REPLY = 1 << 36;

        /// Snooze reminder should be displayed to the user
        const DISPLAY_SNOOZE_REMINDER = 1 << 37;

        // Safeguard against unknown values
        const _ = !0;
    }
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct MessageMetadata {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: MessageId,

    /// TODO: Document this field.
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,

    /// TODO: Document this field.
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,

    /// TODO: Document this field.
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[serde(rename = "BCCList", default)]
    pub bcc_list: Vec<MessageRecipient>,

    /// TODO: Document this field.
    #[serde(rename = "CCList", default)]
    pub cc_list: Vec<MessageRecipient>,

    /// TODO: Document this field.
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,

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
    pub label_ids: Vec<LabelId>,

    /// TODO: Document this field.
    pub num_attachments: u32,

    /// TODO: Document this field.
    pub order: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub sender: MessageSender,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub snooze_time: u64,

    /// TODO: Document this field.
    pub subject: String,

    /// TODO: Document this field.
    pub time: u64,

    /// TODO: Document this field.
    #[serde(default)]
    pub to_list: Vec<MessageRecipient>,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub unread: bool,
}

#[cfg(feature = "mocks")]
impl MessageMetadata {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            id: MessageId::from(""),
            conversation_id: ConversationId::from(""),
            address_id: AddressId::from(""),
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
            sender: MessageSender::default(),
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
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageSender {
    /// TODO: Document this field.
    // TODO: Proper email parsing
    pub address: PrivateEmail,

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
    pub name: PrivateString,
}

/// Recipient of a message.
#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MessageRecipient {
    /// Email of the recipient
    pub address: PrivateEmail,

    /// Whether the recipient is a proton address.
    #[serde(default)]
    #[serde_as(as = "BoolFromInt")]
    pub is_proton: bool,

    /// Display name of the recipient,empty if none.
    pub name: PrivateString,

    /// Name of the address group this recipient belongs too.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

// There is a lot of overlap with this type.
pub type MessageReplyTo = MessageSender;

/// All possible actions sent by API GET settings request
///
/// Found in `MailSettings::MobileSettings::MessageToolbar::Actions` /
///          `MailSettings::MobileSettings::ConversationToolbar::Actions` /
///          `MailSettings::MobileSettings::ListToolbar::Actions`
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileAction {
    Archive,
    Forward,
    Label,
    Move,
    Print,
    Remind,
    Reply,
    ReportPhishing,
    SaveAttachments,
    #[serde(rename = "save_pdf")]
    SavePDF,
    SenderEmails,
    Snooze,
    Spam,
    ToggleLight,
    ToggleRead,
    ToggleStar,
    Trash,
    ViewHeaders,
    #[serde(rename = "view_html")]
    ViewHTML,
    /// For forward compatibility with unknown actions
    #[serde(untagged)]
    Other(String),
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MobileSetting {
    /// TODO: Document this field.
    #[serde(default)]
    pub actions: Vec<MobileAction>,

    /// TODO: Document this field.
    pub is_custom: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct MobileSettings {
    pub conversation_toolbar: MobileSetting,

    pub list_toolbar: MobileSetting,

    pub message_toolbar: MobileSetting,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
// can't put T:ProtonIdMarker here due to https://github.com/rust-lang/rust/issues/34979
pub struct OperationResult<T> {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: T,

    /// TODO: Document this field.
    #[serde(rename = "Response")]
    pub response: ApiErrorInfo,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct UndoToken {
    /// TODO: Document this field.
    pub token: String,

    /// TODO: Document this field.
    #[serde(rename = "ValidUntil")]
    pub valid_until: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct NewAttachmentResponse {
    /// Attachment id.
    #[serde(rename = "ID")]
    pub id: AttachmentId,
    /// Attachment filename.
    #[serde(rename = "Name")]
    pub file_name: String,
    /// Attachment file size.
    #[serde(rename = "Size")]
    pub file_size: u64,
    /// Attachment disposition.
    pub disposition: Disposition,
    /// Binary asymmetric key packet.
    pub key_packets: KeyPackets,
    /// Optional armored detached signature.
    pub signature: Option<AttachmentSignature>,
    /// Optional armored encrypted message containing binary detached signature.
    pub enc_signature: Option<AttachmentEncryptedSignature>,
    /// Attachment headers.
    pub headers: MessageAttachmentHeaders,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct IncomingDefaultEvent {
    #[serde(rename = "ID")]
    pub id: String,

    pub action: Action,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct IncomingDefault {
    #[serde(rename = "ID")]
    pub id: String,

    /// Which label messages from this address go to
    pub location: IncomingDefaultLocation,

    /// What to do with this response
    #[serde(rename = "Type")]
    pub action: Option<Action>,

    pub email: Option<PrivateEmail>,

    // This is unused
    pub domain: Option<String>,
    // time: Option<u64>,
}

/// Where do messages from a sender go by default. This is handled by the backend, but we sometimes
/// want this informaton for things like banners.
#[derive(Clone, Copy, Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq)]
#[repr(u8)]
pub enum IncomingDefaultLocation {
    /// The messages are allowed and go to inbox
    /// Email marked initially as spam by Proton, but marked as "OK" by the user.
    Inbox = 0,
    /// Marked as spam by the user, next incoming messages goes to spam directly
    Spam = 4,
    /// email address blocked by the user, going to permanent deleted immediately (not to trash, not to spam)
    /// The messages are not received and are deleted automatically
    Blocked = 14,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod message_attachment_headers {
        use super::*;

        #[test]
        fn typical() {
            let actual = serde_json::from_str::<MessageAttachmentHeaders>(
                r#"
                {
                    "content-type": "image/gif",
                    "content-description": "logo.gif",
                    "x-pm-image-width": "128",
                    "x-pm-image-height": "64",
                    "x-pm-content-encryption": "on-delivery",
                    "content-disposition": "attachment; filename=logo.gif",
                    "content-id": "<asdf>"
                }
            "#,
            )
            .unwrap();

            assert_eq!(
                MessageAttachmentHeaders {
                    content_disposition: ContentDisposition::One(
                        "attachment; filename=logo.gif".into()
                    ),
                    content_id: Some("<asdf>".into()),
                    content_transfer_encoding: None,
                    image_height: Some("64".into()),
                    image_width: Some("128".into())
                },
                actual,
            );
        }

        #[test]
        fn with_multiple_content_dispositions() {
            let actual = serde_json::from_str::<MessageAttachmentHeaders>(
                r#"
                {
                    "content-type": "image/gif",
                    "content-description": "logo.gif",
                    "x-pm-image-width": "128",
                    "x-pm-image-height": "64",
                    "x-pm-content-encryption": "on-delivery",
                    "content-disposition": [
                        "attachment; filename=logo.gif",
                        "inline; filename=logo.gif"
                    ],
                    "content-id": "<asdf>"
                }
            "#,
            )
            .unwrap();

            assert_eq!(
                MessageAttachmentHeaders {
                    content_disposition: ContentDisposition::Many(vec![
                        "attachment; filename=logo.gif".into(),
                        "inline; filename=logo.gif".into(),
                    ]),
                    content_id: Some("<asdf>".into()),
                    content_transfer_encoding: None,
                    image_height: Some("64".into()),
                    image_width: Some("128".into())
                },
                actual,
            );
        }
    }
}
