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
//! # Notable exclusions
//!
//! The following types are excluded from export via UniFFI, as they do not need
//! to be used outside of the Rust internals:
//!
//!   - [`ConversationLabel`](proton_core_common::models::ConversationLabel)
//!
//! The following fields are excluded from represented types (in addition to
//! internal database fields):
//!
//!   - [`Conversation::labels`](proton_mail_common::models::Message::label_ids)
//!   - [`Message::body`](proton_mail_common::models::Message::body)
//!   - [`Message::label_ids`](proton_mail_common::models::Message::label_ids)
//!
mod attachment;
mod available_action;
mod system_label;

use crate::core::datatypes::{LabelId, RemoteId};
pub use attachment::*;
pub use available_action::*;
use core::fmt;
use proton_api_mail::services::proton::request_data::MessageMetadataSortMode as RealMessageMetadataSortMode;
use proton_api_mail::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use proton_api_mail::MAX_PAGE_ELEMENT_COUNT_U64;
use proton_mail_common::avatar::AvatarInformation as RealAvatarInformation;
use proton_mail_common::datatypes::{
    AlmostAllMail as RealAlmostAllMail, AttachmentMetadata as RealAttachmentMetadata,
    ComposerDirection as RealComposerDirection, ComposerMode as RealComposerMode,
    ConversationCount as RealConversationCount, CustomLabel as RealCustomLabel,
    Disposition as RealDisposition, EncryptedMessageBody as RealEncryptedMessageBody,
    LabelColor as RealLabelColor, LabelType as RealLabelType, MessageAddress as RealMessageAddress,
    MessageAddresses as RealMessageAddresses, MessageAttachment as RealMessageAttachment,
    MessageAttachmentHeaders as RealMessageAttachmentHeaders,
    MessageAttachmentInfo as RealMessageAttachmentInfo,
    MessageAttachmentInfos as RealMessageAttachmentInfos,
    MessageAttachments as RealMessageAttachments, MessageButtons as RealMessageButtons,
    MessageCount as RealMessageCount, MessageFlags as RealMessageFlags, MimeType as RealMimeType,
    MobileSetting as RealMobileSetting, MobileSettings as RealMobileSettings,
    NextMessageOnMove as RealNextMessageOnMove, ParsedHeaderValue as RealParsedHeaderValue,
    ParsedHeaders as RealParsedHeaders, PgpScheme as RealPgpScheme, PmSignature as RealPmSignature,
    RemoteIds as RealRemoteIds, ShowImages as RealShowImages, ShowMoved as RealShowMoved,
    SpamAction as RealSpamAction, SwipeAction as RealSwipeAction, SystemLabel as RealSystemLabel,
    ViewLayout as RealViewLayout, ViewMode as RealViewMode,
};
use proton_mail_common::datatypes::{
    ContextualConversation, ExclusiveLocation as RealExclusiveLocation,
};
use proton_mail_common::decrypted_message;
use proton_mail_common::models::{
    Label as RealLabel, MailSettings as RealMailSettings, Message as RealMessage,
    MessageBodyMetadata as RealMessageBodyMetadata,
};
use serde_json::to_string as to_json_string;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
pub use system_label::*;
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

impl From<AlmostAllMail> for RealAlmostAllMail {
    fn from(value: AlmostAllMail) -> Self {
        match value {
            AlmostAllMail::AllMail => RealAlmostAllMail::AllMail,
            AlmostAllMail::AlmostAllMail => RealAlmostAllMail::AlmostAllMail,
        }
    }
}

impl From<RealAlmostAllMail> for AlmostAllMail {
    fn from(value: RealAlmostAllMail) -> Self {
        match value {
            RealAlmostAllMail::AllMail => AlmostAllMail::AllMail,
            RealAlmostAllMail::AlmostAllMail => AlmostAllMail::AlmostAllMail,
        }
    }
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

impl From<ComposerDirection> for RealComposerDirection {
    fn from(value: ComposerDirection) -> Self {
        match value {
            ComposerDirection::LeftToRight => RealComposerDirection::LeftToRight,
            ComposerDirection::RightToLeft => RealComposerDirection::RightToLeft,
        }
    }
}

impl From<RealComposerDirection> for ComposerDirection {
    fn from(value: RealComposerDirection) -> Self {
        match value {
            RealComposerDirection::LeftToRight => ComposerDirection::LeftToRight,
            RealComposerDirection::RightToLeft => ComposerDirection::RightToLeft,
        }
    }
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

impl From<ComposerMode> for RealComposerMode {
    fn from(value: ComposerMode) -> Self {
        match value {
            ComposerMode::Normal => RealComposerMode::Normal,
            ComposerMode::Maximized => RealComposerMode::Maximized,
        }
    }
}

impl From<RealComposerMode> for ComposerMode {
    fn from(value: RealComposerMode) -> Self {
        match value {
            RealComposerMode::Normal => ComposerMode::Normal,
            RealComposerMode::Maximized => ComposerMode::Maximized,
        }
    }
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

impl From<Disposition> for RealDisposition {
    fn from(value: Disposition) -> Self {
        match value {
            Disposition::Attachment => RealDisposition::Attachment,
            Disposition::Inline => RealDisposition::Inline,
        }
    }
}

impl From<RealDisposition> for Disposition {
    fn from(value: RealDisposition) -> Self {
        match value {
            RealDisposition::Attachment => Disposition::Attachment,
            RealDisposition::Inline => Disposition::Inline,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ExclusiveLocation {
    Inbox,
    Trash,
    Archive,
    Spam,
    Snoozed,
    Scheduled,
    Outbox,
    Custom {
        name: String,
        local_id: u64,
        color: LabelColor,
    },
}

impl From<ExclusiveLocation> for RealExclusiveLocation {
    fn from(value: ExclusiveLocation) -> Self {
        match value {
            ExclusiveLocation::Inbox => RealExclusiveLocation::System(RealSystemLabel::Inbox),
            ExclusiveLocation::Trash => RealExclusiveLocation::System(RealSystemLabel::Trash),
            ExclusiveLocation::Archive => RealExclusiveLocation::System(RealSystemLabel::Archive),
            ExclusiveLocation::Spam => RealExclusiveLocation::System(RealSystemLabel::Spam),
            ExclusiveLocation::Snoozed => RealExclusiveLocation::System(RealSystemLabel::Snoozed),
            ExclusiveLocation::Scheduled => {
                RealExclusiveLocation::System(RealSystemLabel::Scheduled)
            }
            ExclusiveLocation::Outbox => RealExclusiveLocation::System(RealSystemLabel::Outbox),
            ExclusiveLocation::Custom {
                name,
                local_id,
                color,
            } => RealExclusiveLocation::Custom {
                name,
                local_id: local_id.into(),
                color: color.into(),
            },
        }
    }
}

impl From<RealExclusiveLocation> for ExclusiveLocation {
    fn from(value: RealExclusiveLocation) -> Self {
        match value {
            RealExclusiveLocation::System(RealSystemLabel::Inbox) => ExclusiveLocation::Inbox,
            RealExclusiveLocation::System(RealSystemLabel::Trash) => ExclusiveLocation::Trash,
            RealExclusiveLocation::System(RealSystemLabel::Archive) => ExclusiveLocation::Archive,
            RealExclusiveLocation::System(RealSystemLabel::Spam) => ExclusiveLocation::Spam,
            RealExclusiveLocation::System(RealSystemLabel::Snoozed) => ExclusiveLocation::Snoozed,
            RealExclusiveLocation::System(RealSystemLabel::Scheduled) => {
                ExclusiveLocation::Scheduled
            }
            RealExclusiveLocation::System(RealSystemLabel::Outbox) => ExclusiveLocation::Outbox,
            RealExclusiveLocation::Custom {
                name,
                local_id,
                color,
            } => ExclusiveLocation::Custom {
                name,
                local_id: local_id.into(),
                color: color.into(),
            },
            RealExclusiveLocation::System(_) => unreachable!(),
        }
    }
}

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

impl Display for LabelType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System => write!(f, "System"),
        }
    }
}

impl From<LabelType> for RealLabelType {
    fn from(value: LabelType) -> Self {
        match value {
            LabelType::Label => RealLabelType::Label,
            LabelType::ContactGroup => RealLabelType::ContactGroup,
            LabelType::Folder => RealLabelType::Folder,
            LabelType::System => RealLabelType::System,
        }
    }
}

impl From<RealLabelType> for LabelType {
    fn from(value: RealLabelType) -> Self {
        match value {
            RealLabelType::Label => LabelType::Label,
            RealLabelType::ContactGroup => LabelType::ContactGroup,
            RealLabelType::Folder => LabelType::Folder,
            RealLabelType::System => LabelType::System,
        }
    }
}

/// This enum is extended version of the `LabelType` enum. It contains additional
/// information regarding the system label type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum LabelDescription {
    Label,
    ContactGroup,
    Folder,

    /// System field contain information about the system label type.
    /// SystemLabel main purpose is to determine the type of the system label.
    /// It is required for localization in the sidebar & dropdowns.
    /// The information is optional as we cannot forsee all possible system labels.
    System(Option<SystemLabel>),
}

impl LabelDescription {
    #[must_use]
    pub fn new(label: &RealLabel) -> Self {
        match label.label_type {
            RealLabelType::Label => LabelDescription::Label,
            RealLabelType::ContactGroup => LabelDescription::ContactGroup,
            RealLabelType::Folder => LabelDescription::Folder,
            RealLabelType::System => LabelDescription::System(SystemLabel::new(label)),
        }
    }
}

impl Display for LabelDescription {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System(_) => write!(f, "System"),
        }
    }
}

impl From<LabelDescription> for RealLabelType {
    fn from(value: LabelDescription) -> Self {
        match value {
            LabelDescription::Label => RealLabelType::Label,
            LabelDescription::ContactGroup => RealLabelType::ContactGroup,
            LabelDescription::Folder => RealLabelType::Folder,
            LabelDescription::System(_) => RealLabelType::System,
        }
    }
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

impl From<MessageButtons> for RealMessageButtons {
    fn from(value: MessageButtons) -> Self {
        match value {
            MessageButtons::ReadFirst => RealMessageButtons::ReadFirst,
            MessageButtons::UnreadFirst => RealMessageButtons::UnreadFirst,
        }
    }
}

impl From<RealMessageButtons> for MessageButtons {
    fn from(value: RealMessageButtons) -> Self {
        match value {
            RealMessageButtons::ReadFirst => MessageButtons::ReadFirst,
            RealMessageButtons::UnreadFirst => MessageButtons::UnreadFirst,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum MessageMetadataSortMode {
    /// TODO: Document this variant.
    Time,

    /// TODO: Document this variant.
    Size,

    /// TODO: Document this variant.
    ID,
}

impl From<MessageMetadataSortMode> for RealMessageMetadataSortMode {
    fn from(value: MessageMetadataSortMode) -> Self {
        match value {
            MessageMetadataSortMode::Time => RealMessageMetadataSortMode::Time,
            MessageMetadataSortMode::Size => RealMessageMetadataSortMode::Size,
            MessageMetadataSortMode::ID => RealMessageMetadataSortMode::ID,
        }
    }
}

impl From<RealMessageMetadataSortMode> for MessageMetadataSortMode {
    fn from(value: RealMessageMetadataSortMode) -> Self {
        match value {
            RealMessageMetadataSortMode::Time => MessageMetadataSortMode::Time,
            RealMessageMetadataSortMode::Size => MessageMetadataSortMode::Size,
            RealMessageMetadataSortMode::ID => MessageMetadataSortMode::ID,
        }
    }
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

impl From<MimeType> for RealMimeType {
    fn from(value: MimeType) -> Self {
        match value {
            MimeType::ApplicationJson => RealMimeType::ApplicationJson,
            MimeType::ApplicationPdf => RealMimeType::ApplicationPdf,
            MimeType::MessageRfc822 => RealMimeType::MessageRfc822,
            MimeType::MultipartMixed => RealMimeType::MultipartMixed,
            MimeType::MultipartRelated => RealMimeType::MultipartRelated,
            MimeType::TextHtml => RealMimeType::TextHtml,
            MimeType::TextPlain => RealMimeType::TextPlain,
        }
    }
}

impl From<RealMimeType> for MimeType {
    fn from(value: RealMimeType) -> Self {
        match value {
            RealMimeType::ApplicationJson => MimeType::ApplicationJson,
            RealMimeType::ApplicationPdf => MimeType::ApplicationPdf,
            RealMimeType::MessageRfc822 => MimeType::MessageRfc822,
            RealMimeType::MultipartMixed => MimeType::MultipartMixed,
            RealMimeType::MultipartRelated => MimeType::MultipartRelated,
            RealMimeType::TextHtml => MimeType::TextHtml,
            RealMimeType::TextPlain => MimeType::TextPlain,
        }
    }
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

impl From<NextMessageOnMove> for RealNextMessageOnMove {
    fn from(value: NextMessageOnMove) -> Self {
        match value {
            NextMessageOnMove::DisabledExplicit => RealNextMessageOnMove::DisabledExplicit,
            NextMessageOnMove::DisabledImplicit => RealNextMessageOnMove::DisabledImplicit,
            NextMessageOnMove::EnabledExplicit => RealNextMessageOnMove::EnabledExplicit,
        }
    }
}

impl From<RealNextMessageOnMove> for NextMessageOnMove {
    fn from(value: RealNextMessageOnMove) -> Self {
        match value {
            RealNextMessageOnMove::DisabledExplicit => NextMessageOnMove::DisabledExplicit,
            RealNextMessageOnMove::DisabledImplicit => NextMessageOnMove::DisabledImplicit,
            RealNextMessageOnMove::EnabledExplicit => NextMessageOnMove::EnabledExplicit,
        }
    }
}

/// A message parsed header value can either be a string or an array of strings.
#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum ParsedHeaderValue {
    /// TODO: Document this variant.
    Array(Vec<String>),

    /// TODO: Document this variant.
    String(String),
}

impl From<ParsedHeaderValue> for RealParsedHeaderValue {
    fn from(value: ParsedHeaderValue) -> Self {
        match value {
            ParsedHeaderValue::Array(array) => RealParsedHeaderValue::Array(array),
            ParsedHeaderValue::String(string) => RealParsedHeaderValue::String(string),
        }
    }
}

impl From<RealParsedHeaderValue> for ParsedHeaderValue {
    fn from(value: RealParsedHeaderValue) -> Self {
        match value {
            RealParsedHeaderValue::Array(array) => ParsedHeaderValue::Array(array),
            RealParsedHeaderValue::String(string) => ParsedHeaderValue::String(string),
        }
    }
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

impl From<PgpScheme> for RealPgpScheme {
    fn from(value: PgpScheme) -> Self {
        match value {
            PgpScheme::Inline => RealPgpScheme::Inline,
            PgpScheme::Mime => RealPgpScheme::Mime,
        }
    }
}

impl From<RealPgpScheme> for PgpScheme {
    fn from(value: RealPgpScheme) -> Self {
        match value {
            RealPgpScheme::Inline => PgpScheme::Inline,
            RealPgpScheme::Mime => PgpScheme::Mime,
        }
    }
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

impl From<PmSignature> for RealPmSignature {
    fn from(value: PmSignature) -> Self {
        match value {
            PmSignature::Disabled => RealPmSignature::Disabled,
            PmSignature::Enabled => RealPmSignature::Enabled,
            PmSignature::EnabledLocked => RealPmSignature::EnabledLocked,
        }
    }
}

impl From<RealPmSignature> for PmSignature {
    fn from(value: RealPmSignature) -> Self {
        match value {
            RealPmSignature::Disabled => PmSignature::Disabled,
            RealPmSignature::Enabled => PmSignature::Enabled,
            RealPmSignature::EnabledLocked => PmSignature::EnabledLocked,
        }
    }
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

impl From<ShowImages> for RealShowImages {
    fn from(value: ShowImages) -> Self {
        match value {
            ShowImages::DoNotAutoLoad => RealShowImages::DoNotAutoLoad,
            ShowImages::AutoLoadRemote => RealShowImages::AutoLoadRemote,
            ShowImages::AutoLoadEmbedded => RealShowImages::AutoLoadEmbedded,
            ShowImages::AutoLoadBoth => RealShowImages::AutoLoadBoth,
        }
    }
}

impl From<RealShowImages> for ShowImages {
    fn from(value: RealShowImages) -> Self {
        match value {
            RealShowImages::DoNotAutoLoad => ShowImages::DoNotAutoLoad,
            RealShowImages::AutoLoadRemote => ShowImages::AutoLoadRemote,
            RealShowImages::AutoLoadEmbedded => ShowImages::AutoLoadEmbedded,
            RealShowImages::AutoLoadBoth => ShowImages::AutoLoadBoth,
        }
    }
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

impl From<ShowMoved> for RealShowMoved {
    fn from(value: ShowMoved) -> Self {
        match value {
            ShowMoved::DoNotKeep => RealShowMoved::DoNotKeep,
            ShowMoved::KeepInDrafts => RealShowMoved::KeepInDrafts,
            ShowMoved::KeepInSent => RealShowMoved::KeepInSent,
            ShowMoved::KeepBoth => RealShowMoved::KeepBoth,
        }
    }
}

impl From<RealShowMoved> for ShowMoved {
    fn from(value: RealShowMoved) -> Self {
        match value {
            RealShowMoved::DoNotKeep => ShowMoved::DoNotKeep,
            RealShowMoved::KeepInDrafts => ShowMoved::KeepInDrafts,
            RealShowMoved::KeepInSent => ShowMoved::KeepInSent,
            RealShowMoved::KeepBoth => ShowMoved::KeepBoth,
        }
    }
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

impl From<SpamAction> for RealSpamAction {
    fn from(value: SpamAction) -> Self {
        match value {
            SpamAction::DoNothing => RealSpamAction::DoNothing,
            SpamAction::UnsubscribeWithOneClick => RealSpamAction::UnsubscribeWithOneClick,
        }
    }
}

impl From<RealSpamAction> for SpamAction {
    fn from(value: RealSpamAction) -> Self {
        match value {
            RealSpamAction::DoNothing => SpamAction::DoNothing,
            RealSpamAction::UnsubscribeWithOneClick => SpamAction::UnsubscribeWithOneClick,
        }
    }
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

impl From<SwipeAction> for RealSwipeAction {
    fn from(value: SwipeAction) -> Self {
        match value {
            SwipeAction::Trash => RealSwipeAction::Trash,
            SwipeAction::Spam => RealSwipeAction::Spam,
            SwipeAction::Star => RealSwipeAction::Star,
            SwipeAction::Archive => RealSwipeAction::Archive,
            SwipeAction::MarkAsRead => RealSwipeAction::MarkAsRead,
        }
    }
}

impl From<RealSwipeAction> for SwipeAction {
    fn from(value: RealSwipeAction) -> Self {
        match value {
            RealSwipeAction::Trash => SwipeAction::Trash,
            RealSwipeAction::Spam => SwipeAction::Spam,
            RealSwipeAction::Star => SwipeAction::Star,
            RealSwipeAction::Archive => SwipeAction::Archive,
            RealSwipeAction::MarkAsRead => SwipeAction::MarkAsRead,
        }
    }
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

impl From<ViewLayout> for RealViewLayout {
    fn from(value: ViewLayout) -> Self {
        match value {
            ViewLayout::Column => RealViewLayout::Column,
            ViewLayout::Row => RealViewLayout::Row,
        }
    }
}

impl From<RealViewLayout> for ViewLayout {
    fn from(value: RealViewLayout) -> Self {
        match value {
            RealViewLayout::Column => ViewLayout::Column,
            RealViewLayout::Row => ViewLayout::Row,
        }
    }
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

impl From<ViewMode> for RealViewMode {
    fn from(value: ViewMode) -> Self {
        match value {
            ViewMode::Conversations => RealViewMode::Conversations,
            ViewMode::Messages => RealViewMode::Messages,
        }
    }
}

impl From<RealViewMode> for ViewMode {
    fn from(value: RealViewMode) -> Self {
        match value {
            RealViewMode::Conversations => ViewMode::Conversations,
            RealViewMode::Messages => ViewMode::Messages,
        }
    }
}

//  STRUCTS
//==============================================================================

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AttachmentMetadata {
    /// Local attachment id
    pub local_id: u64,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// Attachment mime type is a flexible type that can be used to categorize
    /// media types. It allows any media type to be used, but also has a
    /// category field to allow to pick aprpopriate icons for the media type.
    pub mime_type: AttachmentMimeType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

impl From<RealAttachmentMetadata> for AttachmentMetadata {
    fn from(value: RealAttachmentMetadata) -> Self {
        AttachmentMetadata {
            local_id: value.local_id.unwrap().into(),
            disposition: value.disposition.into(),
            mime_type: value.mime_type.into(),
            name: value.filename,
            size: value.size,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AvatarInformation {
    /// TODO: Document this field.
    pub text: String,

    /// TODO: Document this field.
    pub color: String,
}

impl From<AvatarInformation> for RealAvatarInformation {
    fn from(value: AvatarInformation) -> Self {
        RealAvatarInformation {
            text: value.text,
            color: value.color,
        }
    }
}

impl From<RealAvatarInformation> for AvatarInformation {
    fn from(value: RealAvatarInformation) -> Self {
        AvatarInformation {
            text: value.text,
            color: value.color,
        }
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
    pub local_id: u64,

    /// Metadata for all attachments in this conversation.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// TODO: Document this field.
    pub display_snooze_reminder: bool,

    /// Exclusive location of the [`Conversation`] (e.g. Inbox, Archive, Outbox
    /// etc.).
    pub exclusive_location: Option<ExclusiveLocation>,

    /// When this conversation expires.
    pub expiration_time: u64,

    /// Whether the conversation is starred.
    pub is_starred: bool,

    /// Number of attachments in this conversation.
    pub num_attachments: u64,

    /// Number of messages in this conversation.
    pub num_messages: u64,

    /// Number of unread messages in this conversation.
    pub num_unread: u64,

    /// Display order in the list.
    pub display_order: u64,

    /// All recipients from messages in this conversation.
    pub recipients: MessageAddresses,

    /// All senders from messages in this conversation.
    pub senders: MessageAddresses,

    /// Total size of all the messages in this conversation.
    pub size: u64,

    /// Subject of the conversation.
    pub subject: String,

    /// Time of the last received message in this conversation.
    pub time: u64,
}

impl From<ContextualConversation> for Conversation {
    fn from(value: ContextualConversation) -> Self {
        Self {
            local_id: value.local_id.into(),
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(Into::into)
                .collect(),
            custom_labels: value.custom_labels.into_iter().map(Into::into).collect(),
            display_order: value.display_order,
            display_snooze_reminder: value.display_snooze_reminder,
            exclusive_location: value.exclusive_location.map(Into::into),
            expiration_time: value.expiration_time,
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            recipients: value.recipients.into(),
            senders: value.senders.into(),
            size: value.size,
            is_starred: value.is_starred,
            subject: value.subject,
            time: value.time,
        }
    }
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

impl From<RealConversationCount> for ConversationCount {
    fn from(value: RealConversationCount) -> Self {
        ConversationCount {
            label_id: value.label_id.into(),
            total: value.total,
            unread: value.unread,
        }
    }
}

/// Parameters to filter/search conversations with a given criteria.
#[derive(Clone, Debug, SmartDefault, UniffiRecord)]
pub struct ConversationSearchOptions {
    /// Address ID to filter on.
    pub address_id: Option<RemoteId>,

    /// If `true`, only return conversations which have attachments. If `false`,
    /// only return conversations which have no attachments.
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// UNIX timestamp to filter conversations earlier than timestamp.
    pub begin: Option<u64>,

    /// Return only conversations newer, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `begin`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    pub begin_id: Option<RemoteId>,

    /// If `true`, return results in descending order rather than ascending.
    pub desc: Option<bool>,

    /// UNIX timestamp to filter conversations later than timestamp.
    pub end: Option<u64>,

    /// Return only conversations older, in creation time (NOT timestamp), than
    /// the specified conversation ID if timestamp = `end`.
    // TODO: Improve the documentation above, as it doesn't make total sense.
    pub end_id: Option<RemoteId>,

    /// External ID to filter on.
    pub external_id: Option<RemoteId>,

    /// Keyword search of `From` field.
    pub from: Option<String>,

    /// Conversation IDs to filter on.
    pub ids: Option<Vec<RemoteId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label ID to filter on.
    pub label_id: Option<RemoteId>,

    /// The number of conversations to return.
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Sort the results by one of the sorting modes.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search of `Subject` field.
    pub subject: Option<String>,

    /// If `true`, only return conversations which have unread messages. If
    /// `false`, only return conversations which have all messages read.
    pub unread: Option<bool>,
}

impl From<ConversationSearchOptions> for GetConversationsOptions {
    fn from(value: ConversationSearchOptions) -> Self {
        GetConversationsOptions {
            address_id: value.address_id.map(Into::into),
            attachments: value.attachments,
            auto_wildcard: value.auto_wildcard,
            begin: value.begin,
            begin_id: value.begin_id.map(Into::into),
            desc: value.desc,
            end: value.end,
            end_id: value.end_id.map(Into::into),
            external_id: value.external_id.map(Into::into),
            from: value.from,
            ids: value
                .ids
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            keyword: value.keyword,
            label_id: value.label_id.map(Into::into),
            limit: value.limit,
            page: value.page,
            page_size: value.page_size,
            recipients: value.recipients,
            sort: value.sort.map(Into::into),
            subject: value.subject,
            unread: value.unread,
        }
    }
}

impl From<GetConversationsOptions> for ConversationSearchOptions {
    fn from(value: GetConversationsOptions) -> Self {
        ConversationSearchOptions {
            address_id: value.address_id.map(Into::into),
            attachments: value.attachments,
            auto_wildcard: value.auto_wildcard,
            begin: value.begin,
            begin_id: value.begin_id.map(Into::into),
            desc: value.desc,
            end: value.end,
            end_id: value.end_id.map(Into::into),
            external_id: value.external_id.map(Into::into),
            from: value.from,
            ids: value
                .ids
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            keyword: value.keyword,
            label_id: value.label_id.map(Into::into),
            limit: value.limit,
            page: value.page,
            page_size: value.page_size,
            recipients: value.recipients,
            sort: value.sort.map(Into::into),
            subject: value.subject,
            unread: value.unread,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct EncryptedMessageBody {
    /// TODO: Document this field.
    pub encrypted_body: String,

    /// TODO: Document this field.
    pub metadata: MessageBodyMetadata,
}

impl From<RealEncryptedMessageBody> for EncryptedMessageBody {
    fn from(value: RealEncryptedMessageBody) -> Self {
        EncryptedMessageBody {
            encrypted_body: value.encrypted_body,
            metadata: value.metadata.into(),
        }
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
    pub local_id: u64,

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
    pub label_description: LabelDescription,

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

impl From<RealLabel> for Label {
    fn from(value: RealLabel) -> Self {
        let label_description = LabelDescription::new(&value);

        Label {
            local_id: value.local_id.unwrap().into(),
            local_parent_id: value.local_parent_id.map(Into::into),
            remote_parent_id: value.remote_parent_id.map(Into::into),
            color: value.color.into(),
            display: value.display,
            expanded: value.expanded,
            initialized_conv: value.initialized_conv,
            initialized_msg: value.initialized_msg,
            label_description,
            name: value.name,
            notify: value.notify,
            display_order: value.display_order,
            path: value.path,
            sticky: value.sticky,
            total_conv: value.total_conv,
            total_msg: value.total_msg,
            unread_conv: value.unread_conv,
            unread_msg: value.unread_msg,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct LabelColor {
    value: String,
}

impl Display for LabelColor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<LabelColor> for RealLabelColor {
    fn from(value: LabelColor) -> Self {
        RealLabelColor::from(value.to_string())
    }
}

impl From<RealLabelColor> for LabelColor {
    fn from(value: RealLabelColor) -> Self {
        LabelColor::from(value.to_string())
    }
}

impl From<String> for LabelColor {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl From<&str> for LabelColor {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
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
    local_id: u64,

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

impl From<MailSettings> for RealMailSettings {
    fn from(value: MailSettings) -> Self {
        RealMailSettings {
            local_id: Some(value.local_id.into()),
            almost_all_mail: value.almost_all_mail.into(),
            attach_public_key: value.attach_public_key,
            auto_delete_spam_and_trash_days: value.auto_delete_spam_and_trash_days,
            auto_save_contacts: value.auto_save_contacts,
            block_sender_confirmation: value.block_sender_confirmation,
            composer_mode: value.composer_mode.into(),
            confirm_link: value.confirm_link,
            delay_send_seconds: value.delay_send_seconds,
            display_name: value.display_name,
            draft_mime_type: value.draft_mime_type.into(),
            enable_folder_color: value.enable_folder_color,
            font_face: value.font_face,
            hide_remote_images: value.hide_remote_images,
            hide_sender_images: value.hide_sender_images,
            image_proxy: value.image_proxy,
            inherit_parent_folder_color: value.inherit_parent_folder_color,
            message_buttons: value.message_buttons.into(),
            mobile_settings: value.mobile_settings.map(Into::into),
            next_message_on_move: value.next_message_on_move.map(Into::into),
            num_message_per_page: value.num_message_per_page,
            pgp_scheme: value.pgp_scheme.into(),
            pm_signature: value.pm_signature.into(),
            pm_signature_referral_link: value.pm_signature_referral_link,
            prompt_pin: value.prompt_pin,
            receive_mime_type: value.receive_mime_type.into(),
            right_to_left: value.right_to_left.into(),
            shortcuts: value.shortcuts,
            show_images: value.show_images.into(),
            show_mime_type: value.show_mime_type.into(),
            show_moved: value.show_moved.into(),
            sign: value.sign,
            signature: value.signature,
            spam_action: value.spam_action.map(Into::into),
            sticky_labels: value.sticky_labels,
            submission_access: value.submission_access,
            swipe_left: value.swipe_left.into(),
            swipe_right: value.swipe_right.into(),
            theme: value.theme,
            view_layout: value.view_layout.into(),
            view_mode: value.view_mode.into(),
            row_id: None,
            stash: None,
        }
    }
}

impl From<RealMailSettings> for MailSettings {
    fn from(value: RealMailSettings) -> Self {
        MailSettings {
            local_id: value.local_id.unwrap().into(),
            almost_all_mail: value.almost_all_mail.into(),
            attach_public_key: value.attach_public_key,
            auto_delete_spam_and_trash_days: value.auto_delete_spam_and_trash_days,
            auto_save_contacts: value.auto_save_contacts,
            block_sender_confirmation: value.block_sender_confirmation,
            composer_mode: value.composer_mode.into(),
            confirm_link: value.confirm_link,
            delay_send_seconds: value.delay_send_seconds,
            display_name: value.display_name,
            draft_mime_type: value.draft_mime_type.into(),
            enable_folder_color: value.enable_folder_color,
            font_face: value.font_face,
            hide_remote_images: value.hide_remote_images,
            hide_sender_images: value.hide_sender_images,
            image_proxy: value.image_proxy,
            inherit_parent_folder_color: value.inherit_parent_folder_color,
            message_buttons: value.message_buttons.into(),
            mobile_settings: value.mobile_settings.map(Into::into),
            next_message_on_move: value.next_message_on_move.map(Into::into),
            num_message_per_page: value.num_message_per_page,
            pgp_scheme: value.pgp_scheme.into(),
            pm_signature: value.pm_signature.into(),
            pm_signature_referral_link: value.pm_signature_referral_link,
            prompt_pin: value.prompt_pin,
            receive_mime_type: value.receive_mime_type.into(),
            right_to_left: value.right_to_left.into(),
            shortcuts: value.shortcuts,
            show_images: value.show_images.into(),
            show_mime_type: value.show_mime_type.into(),
            show_moved: value.show_moved.into(),
            sign: value.sign,
            signature: value.signature,
            spam_action: value.spam_action.map(Into::into),
            sticky_labels: value.sticky_labels,
            submission_access: value.submission_access,
            swipe_left: value.swipe_left.into(),
            swipe_right: value.swipe_right.into(),
            theme: value.theme,
            view_layout: value.view_layout.into(),
            view_mode: value.view_mode.into(),
        }
    }
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
    pub local_id: u64,

    /// TODO: Document this field.
    pub local_conversation_id: u64,

    /// TODO: Document this field.
    pub remote_conversation_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub address_id: RemoteId,

    /// Attachment metadata associated with this message.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    pub bcc_list: MessageAddresses,

    /// TODO: Document this field.
    pub cc_list: MessageAddresses,

    /// TODO: Document this field.
    pub deleted: bool,

    /// Exclusive location of the [`Message`] (e.g. Inbox, Archive, Outbox
    /// etc.).
    pub exclusive_location: Option<ExclusiveLocation>,

    /// TODO: Document this field.
    pub expiration_time: u64,

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

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// Whether the message is starred.
    pub starred: bool,
}

impl From<RealMessage> for Message {
    fn from(value: RealMessage) -> Self {
        let starred = value.is_starred();

        Message {
            local_id: value.local_id.unwrap().into(),
            local_conversation_id: value.local_conversation_id.unwrap().into(),
            remote_conversation_id: value.remote_conversation_id.map(Into::into),
            address_id: value.address_id.into(),
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(Into::into)
                .collect(),
            bcc_list: value.bcc_list.into(),
            cc_list: value.cc_list.into(),
            deleted: value.deleted,
            exclusive_location: value.exclusive_location.map(Into::into),
            expiration_time: value.expiration_time,
            header: value.header,
            flags: value.flags.into(),
            is_forwarded: value.is_forwarded,
            is_replied: value.is_replied,
            is_replied_all: value.is_replied_all,
            mime_type: value.mime_type.into(),
            num_attachments: value.num_attachments,
            display_order: value.display_order,
            parsed_headers: value.parsed_headers.into(),
            reply_tos: value.reply_tos.into(),
            sender: value.sender.into(),
            size: value.size,
            snooze_time: value.snooze_time,
            subject: value.subject,
            time: value.time,
            to_list: value.to_list.into(),
            unread: value.unread,
            custom_labels: value.custom_labels.into_iter().map(Into::into).collect(),
            starred,
        }
    }
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

impl From<MessageAddress> for RealMessageAddress {
    fn from(value: MessageAddress) -> Self {
        RealMessageAddress {
            address: value.address,
            bimi_selector: value.bimi_selector,
            display_sender_image: value.display_sender_image,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            name: value.name,
        }
    }
}

impl From<RealMessageAddress> for MessageAddress {
    fn from(value: RealMessageAddress) -> Self {
        MessageAddress {
            address: value.address,
            bimi_selector: value.bimi_selector,
            display_sender_image: value.display_sender_image,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            name: value.name,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct MessageAddresses {
    pub value: Vec<MessageAddress>,
}

impl From<MessageAddresses> for RealMessageAddresses {
    fn from(value: MessageAddresses) -> Self {
        RealMessageAddresses {
            value: value.value.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RealMessageAddresses> for MessageAddresses {
    fn from(value: RealMessageAddresses) -> Self {
        MessageAddresses {
            value: value.value.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachment {
    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<String>,

    /// TODO: Document this field.
    pub headers: MessageAttachmentHeaders,

    /// TODO: Document this field.
    pub key_packets: String,

    /// Attachment mime type is a flexible type that can be used to categorize
    /// media types. It allows any media type to be used, but also has a
    /// category field to allow to pick aprpopriate icons for the media type.
    pub mime_type: AttachmentMimeType,

    /// TODO: Document this field.
    pub signature: Option<String>,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

impl From<RealMessageAttachment> for MessageAttachment {
    fn from(value: RealMessageAttachment) -> Self {
        MessageAttachment {
            disposition: value.disposition.into(),
            enc_signature: value
                .enc_signature
                .as_deref()
                .map(|v| to_json_string(v).unwrap()),
            headers: value.headers.into(),
            key_packets: to_json_string(&value.key_packets).unwrap(),
            mime_type: value.mime_type.into(),
            signature: value
                .signature
                .as_deref()
                .map(|v| to_json_string(&v).unwrap()),
            name: value.name,
            size: value.size,
        }
    }
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

impl From<MessageAttachmentHeaders> for RealMessageAttachmentHeaders {
    fn from(value: MessageAttachmentHeaders) -> Self {
        RealMessageAttachmentHeaders {
            content_disposition: value.content_disposition,
            content_id: value.content_id,
            content_transfer_encoding: value.content_transfer_encoding,
            image_height: value.image_height,
            image_width: value.image_width,
        }
    }
}

impl From<RealMessageAttachmentHeaders> for MessageAttachmentHeaders {
    fn from(value: RealMessageAttachmentHeaders) -> Self {
        MessageAttachmentHeaders {
            content_disposition: value.content_disposition,
            content_id: value.content_id,
            content_transfer_encoding: value.content_transfer_encoding,
            image_height: value.image_height,
            image_width: value.image_width,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageAttachmentInfo {
    /// TODO: Document this field.
    pub attachment: u32,

    /// TODO: Document this field.
    pub inline: u32,
}

impl From<MessageAttachmentInfo> for RealMessageAttachmentInfo {
    fn from(value: MessageAttachmentInfo) -> Self {
        RealMessageAttachmentInfo {
            attachment: value.attachment,
            inline: value.inline,
        }
    }
}

impl From<RealMessageAttachmentInfo> for MessageAttachmentInfo {
    fn from(value: RealMessageAttachmentInfo) -> Self {
        MessageAttachmentInfo {
            attachment: value.attachment,
            inline: value.inline,
        }
    }
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

impl From<MessageAttachmentInfos> for RealMessageAttachmentInfos {
    fn from(value: MessageAttachmentInfos) -> Self {
        RealMessageAttachmentInfos {
            value: value
                .value
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl From<RealMessageAttachmentInfos> for MessageAttachmentInfos {
    fn from(value: RealMessageAttachmentInfos) -> Self {
        MessageAttachmentInfos {
            value: value
                .value
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
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

impl From<RealMessageAttachments> for MessageAttachments {
    fn from(value: RealMessageAttachments) -> Self {
        MessageAttachments {
            value: value.value.into_iter().map(Into::into).collect(),
        }
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
    pub local_message_id: u64,

    /// TODO: Document this field.
    pub header: String,

    /// TODO: Document this field.
    pub mime_type: MimeType,

    /// TODO: Document this field.
    pub parsed_headers: ParsedHeaders,
}

impl From<RealMessageBodyMetadata> for MessageBodyMetadata {
    fn from(value: RealMessageBodyMetadata) -> Self {
        MessageBodyMetadata {
            local_message_id: value.local_message_id.unwrap().into(),
            header: value.header,
            mime_type: value.mime_type.into(),
            parsed_headers: value.parsed_headers.into(),
        }
    }
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

impl From<RealMessageCount> for MessageCount {
    fn from(value: RealMessageCount) -> Self {
        MessageCount {
            label_id: value.label_id.into(),
            total: value.total,
            unread: value.unread,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Copy, Debug, Eq, PartialEq, UniffiRecord)]
pub struct MessageFlags {
    value: u64,
}

impl From<MessageFlags> for RealMessageFlags {
    fn from(value: MessageFlags) -> Self {
        RealMessageFlags::from_bits_truncate(value.value)
    }
}

impl From<RealMessageFlags> for MessageFlags {
    fn from(value: RealMessageFlags) -> Self {
        MessageFlags {
            value: value.bits(),
        }
    }
}

/// Parameters to filter/search messages with a given criteria.
#[derive(Clone, Debug, SmartDefault, UniffiRecord)]
pub struct MessageSearchOptions {
    /// Filter on address ID.
    pub address_id: Option<RemoteId>,

    /// If `true`, return only messages which have attachments. If `false`,
    /// return only messages which have no attachments.
    pub attachments: Option<bool>,

    /// If `true`, automatically convert simple queries to wildcarded versions,
    /// such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,

    /// Keyword search of `BCC` field.
    pub bcc: Option<String>,

    /// UNIX timestamp to filter messages at or later than timestamp.
    pub begin: Option<u64>,

    /// Return only messages newer, in creation time (NOT timestamp), than
    /// the specified message ID.
    pub begin_id: Option<RemoteId>,

    /// Keyword search of CC field.
    pub cc: Option<String>,

    /// Filter messages by conversation ID.
    pub conversation_id: Option<RemoteId>,

    /// If `true`, sort results descending. If `false`, sort ascending.
    pub desc: Option<bool>,

    /// UNIX timestamp to filter messages at or earlier than timestamp.
    pub end: Option<u64>,

    /// Return only messages older, in creation time (NOT timestamp), than the
    /// specified message ID.
    pub end_id: Option<RemoteId>,

    /// Filter on external ID.
    pub external_id: Option<RemoteId>,

    /// Keyword search `From` field.
    pub from: Option<String>,

    /// Filter on the given message IDs.
    pub ids: Option<Vec<RemoteId>>,

    /// Keyword search of `To`, `CC`, `BCC`, `From`, and `Subject` fields.
    pub keyword: Option<String>,

    /// Label IDs to filter on.
    pub label_id: Option<Vec<RemoteId>>,

    /// The number of messages to return.
    pub limit: Option<u64>,

    /// Page index.
    pub page: u64,

    /// Number of elements per page.
    #[default(MAX_PAGE_ELEMENT_COUNT_U64)]
    pub page_size: u64,

    /// Keyword search of `To`, `CC`, and `BCC` fields.
    pub recipients: Option<Vec<String>>,

    /// Result sort mode.
    pub sort: Option<MessageMetadataSortMode>,

    /// Keyword search `Subject` field.
    pub subject: Option<String>,

    /// Keyword search of `To` field.
    pub to: Option<String>,

    /// If `true`, return only messages which are unread. If `false`, return
    /// only messages which are read.
    pub unread: Option<bool>,
}

impl From<MessageSearchOptions> for GetMessagesOptions {
    fn from(value: MessageSearchOptions) -> Self {
        GetMessagesOptions {
            address_id: value.address_id.map(Into::into),
            attachments: value.attachments,
            auto_wildcard: value.auto_wildcard,
            bcc: value.bcc,
            begin: value.begin,
            begin_id: value.begin_id.map(Into::into),
            cc: value.cc,
            conversation_id: value.conversation_id.map(Into::into),
            desc: value.desc,
            end: value.end,
            end_id: value.end_id.map(Into::into),
            external_id: value.external_id.map(Into::into),
            from: value.from,
            ids: value
                .ids
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            keyword: value.keyword,
            label_id: value
                .label_id
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            limit: value.limit,
            page: value.page,
            page_size: value.page_size,
            recipients: value.recipients,
            sort: value.sort.map(Into::into),
            subject: value.subject,
            to: value.to,
            unread: value.unread,
        }
    }
}

impl From<GetMessagesOptions> for MessageSearchOptions {
    fn from(value: GetMessagesOptions) -> Self {
        MessageSearchOptions {
            address_id: value.address_id.map(Into::into),
            attachments: value.attachments,
            auto_wildcard: value.auto_wildcard,
            bcc: value.bcc,
            begin: value.begin,
            begin_id: value.begin_id.map(Into::into),
            cc: value.cc,
            conversation_id: value.conversation_id.map(Into::into),
            desc: value.desc,
            end: value.end,
            end_id: value.end_id.map(Into::into),
            external_id: value.external_id.map(Into::into),
            from: value.from,
            ids: value
                .ids
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            keyword: value.keyword,
            label_id: value
                .label_id
                .map(|ids| ids.into_iter().map(Into::into).collect()),
            limit: value.limit,
            page: value.page,
            page_size: value.page_size,
            recipients: value.recipients,
            sort: value.sort.map(Into::into),
            subject: value.subject,
            to: value.to,
            unread: value.unread,
        }
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

impl From<MobileSetting> for RealMobileSetting {
    fn from(value: MobileSetting) -> Self {
        RealMobileSetting {
            actions: value.actions,
            is_custom: value.is_custom,
        }
    }
}

impl From<RealMobileSetting> for MobileSetting {
    fn from(value: RealMobileSetting) -> Self {
        MobileSetting {
            actions: value.actions,
            is_custom: value.is_custom,
        }
    }
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

impl From<MobileSettings> for RealMobileSettings {
    fn from(value: MobileSettings) -> Self {
        RealMobileSettings {
            conversation_toolbar: value.conversation_toolbar.into(),
            list_toolbar: value.list_toolbar.into(),
            message_toolbar: value.message_toolbar.into(),
        }
    }
}

impl From<RealMobileSettings> for MobileSettings {
    fn from(value: RealMobileSettings) -> Self {
        MobileSettings {
            conversation_toolbar: value.conversation_toolbar.into(),
            list_toolbar: value.list_toolbar.into(),
            message_toolbar: value.message_toolbar.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct ParsedHeaders {
    pub headers: HashMap<String, String>,
}

impl From<ParsedHeaders> for RealParsedHeaders {
    fn from(value: ParsedHeaders) -> Self {
        RealParsedHeaders {
            headers: value.headers,
        }
    }
}

impl From<RealParsedHeaders> for ParsedHeaders {
    fn from(value: RealParsedHeaders) -> Self {
        ParsedHeaders {
            headers: value.headers,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct RemoteIds {
    pub value: Vec<RemoteId>,
}

impl From<RemoteIds> for RealRemoteIds {
    fn from(value: RemoteIds) -> Self {
        RealRemoteIds {
            value: value.value.iter().map(|id| id.clone().into()).collect(),
        }
    }
}

impl From<RealRemoteIds> for RemoteIds {
    fn from(value: RealRemoteIds) -> Self {
        RemoteIds {
            value: value.value.iter().map(|id| id.clone().into()).collect(),
        }
    }
}

/// Information about [`Label`] of type [`Label`] that are applied
/// to [`Conversation`] or [`Messages`].
#[derive(Debug, Clone, Eq, PartialEq, UniffiRecord)]
pub struct CustomLabel {
    /// Local id of the label
    pub local_id: u64,
    /// Name of the label
    pub name: String,
    /// Color of the label.
    pub color: LabelColor,
}

impl From<RealCustomLabel> for CustomLabel {
    fn from(value: RealCustomLabel) -> Self {
        Self {
            local_id: value.local_id.into(),
            name: value.name,
            color: value.color.into(),
        }
    }
}

/// Enable or disable remote content (images).
#[derive(Debug, Clone, Copy, Default, uniffi::Enum)]
pub enum RemoteContent {
    /// Use whatever is in the user's [`MailSettings`]
    #[default]
    Default,
    /// Override the settings and show images
    Enabled,
    /// Override the settings and don't show images
    Disabled,
}

/// What to do with the blockquote (previous conversation threads)
#[derive(Debug, Clone, Copy, Default, uniffi::Enum)]
pub enum BlockQuote {
    /// Remove the previous conversation.
    #[default]
    Strip,
    /// Don't remove the previous conversation
    Untouched,
}

impl From<decrypted_message::RemoteContent> for RemoteContent {
    fn from(value: decrypted_message::RemoteContent) -> Self {
        use decrypted_message::RemoteContent::{Default, Disabled, Enabled};
        match value {
            Default => Self::Default,
            Enabled => Self::Enabled,
            Disabled => Self::Disabled,
        }
    }
}

impl From<decrypted_message::BlockQuote> for BlockQuote {
    fn from(value: decrypted_message::BlockQuote) -> Self {
        use decrypted_message::BlockQuote::{Strip, Untouched};
        match value {
            Strip => Self::Strip,
            Untouched => Self::Untouched,
        }
    }
}

impl From<RemoteContent> for decrypted_message::RemoteContent {
    fn from(value: RemoteContent) -> Self {
        use decrypted_message::RemoteContent as Rc;
        match value {
            RemoteContent::Default => Rc::Default,
            RemoteContent::Enabled => Rc::Enabled,
            RemoteContent::Disabled => Rc::Disabled,
        }
    }
}

impl From<BlockQuote> for decrypted_message::BlockQuote {
    fn from(value: BlockQuote) -> Self {
        use decrypted_message::BlockQuote as Bq;
        match value {
            BlockQuote::Strip => Bq::Strip,
            BlockQuote::Untouched => Bq::Untouched,
        }
    }
}
