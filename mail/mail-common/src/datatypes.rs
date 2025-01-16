//! Persistent data types for the Proton Mail common library.
//!
//! This module contains various data types used by the Proton Mail common
//! library. Many of these are used by the models in the [`models`](crate::models)
//! module, where they represent child data structures for the models' fields.
//! The models themselves should not be placed in this module.
//!
//! All data types used by [`Model`](stash::macros::Model) fields need to be
//! convertible to and from database-compatible format using [`ToSql`] and
//! [`FromSql`]. They do not generally need to be serializable or
//! deserializable, as they are not used for network communication or any other
//! interchange purpose as a general requirement, and so implementation of
//! [`Serialize`] and [`Deserialize`] is not necessary and may be a sign of a
//! mistake. The exception here is when these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`], as a convenience mechanism. This is
//! notably useful when wanting to store types as JSON in a database field, for
//! instance.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need. Such conversions are usually very simple and indeed in many
//! cases can be done without altering any data in memory.
//!
//! This separation does cause some duplication, but the overlap is not total.
//! The various implementations for the types differ in each place; any logic
//! for the application is in the application types and not the API types; and
//! the distinction allows customisation of how the application deals with and
//! stores its related data. Additionally, it promotes wider usability, as each
//! application that depends upon the API types can interpret and managed them
//! in its own way.
//!
//! Note: The current exception to this organisation rule is that of the data
//! types used for events. These are not saved in the database, and so do not
//! have a related model, and their data types are not placed into this module
//! as they are not related to modelling of persistent data against storage.
//! Hence event data types are placed into the [`events`](crate::events) module.
//!

pub mod attachment;
pub(crate) mod contextual_conversation;
pub(crate) mod exclusive_location;
pub mod labels;
mod read_filter;
mod rollback_item_type;
mod search_options;
mod system_folder;
pub(crate) mod system_label;

pub use contextual_conversation::*;
pub use exclusive_location::ExclusiveLocation;
pub use read_filter::ReadFilter;
pub use rollback_item_type::RollbackItemType;
pub use search_options::SearchOptions;
use stash::stash::Tether;
pub use system_folder::MovableSystemFolder;
pub use system_label::SystemLabel;

use crate::decrypted_message::DecryptedMessageBody;
use crate::draft::recipients::MaybeEmptyString;
use crate::models::{Label, MailSettings, MessageBodyMetadata};
use crate::{AppError, MailUserContext};
use core::fmt;
use proton_api_core::services::proton::common::LabelId;
use proton_api_mail::services::proton::common::{AttachmentId, LabelType as ApiLabelType};
use proton_api_mail::services::proton::response_data::{
    AlmostAllMail as ApiAlmostAllMail, AttachmentMetadata as ApiAttachmentMetadata,
    ComposerDirection as ApiComposerDirection, ComposerMode as ApiComposerMode,
    ConversationCount as ApiConversationCount, Disposition as ApiDisposition,
    MessageAttachment as ApiMessageAttachment,
    MessageAttachmentHeaders as ApiMessageAttachmentHeaders,
    MessageAttachmentInfo as ApiMessageAttachmentInfo, MessageButtons as ApiMessageButtons,
    MessageCount as ApiMessageCount, MessageFlags as ApiMessageFlags,
    MessageRecipient as ApiMessageRecipient, MessageReplyTo as ApiMessageReplyTo,
    MessageSender as ApiMessageSender, MimeType as ApiMimeType, MobileSetting as ApiMobileSetting,
    MobileSettings as ApiMobileSettings, NextMessageOnMove as ApiNextMessageOnMove,
    PgpScheme as ApiPgpScheme, PmSignature as ApiPmSignature, ShowImages as ApiShowImages,
    ShowMoved as ApiShowMoved, SpamAction as ApiSpamAction, SwipeAction as ApiSwipeAction,
    ViewLayout as ApiViewLayout, ViewMode as ApiViewMode,
};
use proton_core_common::datatypes::{AvatarInformation, LocalLabelId};
use proton_crypto_account::keys::{
    EmailMimeType as CryptoMimeType, PGPScheme as CryptoPgpScheme, UnlockedAddressKeys,
};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{
    DecryptableMessage, DecryptedBody, GettablePGPMessage, MessageError,
};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_inbox_mime::ProcessedMessage;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::sql_using_serde;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, warn};
//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum AlmostAllMail {
    /// TODO: Document this variant.
    AllMail = 0,

    /// TODO: Document this variant.
    #[default]
    AlmostAllMail = 1,
}

impl From<ApiAlmostAllMail> for AlmostAllMail {
    fn from(value: ApiAlmostAllMail) -> Self {
        match value {
            ApiAlmostAllMail::AllMail => Self::AllMail,
            ApiAlmostAllMail::AlmostAllMail => Self::AlmostAllMail,
        }
    }
}

impl FromSql for AlmostAllMail {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::AllMail),
            1 => Ok(Self::AlmostAllMail),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for AlmostAllMail {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ComposerDirection {
    /// TODO: Document this variant.
    #[default]
    LeftToRight = 0,

    /// TODO: Document this variant.
    RightToLeft = 1,
}

impl From<ApiComposerDirection> for ComposerDirection {
    fn from(value: ApiComposerDirection) -> Self {
        match value {
            ApiComposerDirection::LeftToRight => Self::LeftToRight,
            ApiComposerDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

impl FromSql for ComposerDirection {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::LeftToRight),
            1 => Ok(Self::RightToLeft),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ComposerDirection {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ComposerMode {
    /// TODO: Document this variant.
    #[default]
    Normal = 0,

    /// TODO: Document this variant.
    Maximized = 1,
}

impl From<ApiComposerMode> for ComposerMode {
    fn from(value: ApiComposerMode) -> Self {
        match value {
            ApiComposerMode::Normal => Self::Normal,
            ApiComposerMode::Maximized => Self::Maximized,
        }
    }
}

impl FromSql for ComposerMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Maximized),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ComposerMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[repr(u8)]
pub enum Disposition {
    /// TODO: Document this variant.
    Attachment = 1,

    /// TODO: Document this variant.
    Inline = 2,
}

impl From<ApiDisposition> for Disposition {
    fn from(value: ApiDisposition) -> Self {
        match value {
            ApiDisposition::Attachment => Self::Attachment,
            ApiDisposition::Inline => Self::Inline,
        }
    }
}

impl FromSql for Disposition {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Attachment),
            2 => Ok(Self::Inline),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for Disposition {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System => write!(f, "System"),
        }
    }
}

impl From<ApiLabelType> for LabelType {
    fn from(value: ApiLabelType) -> Self {
        match value {
            ApiLabelType::Label => Self::Label,
            ApiLabelType::ContactGroup => Self::ContactGroup,
            ApiLabelType::Folder => Self::Folder,
            ApiLabelType::System => Self::System,
        }
    }
}

impl From<LabelType> for ApiLabelType {
    fn from(value: LabelType) -> Self {
        match value {
            LabelType::Label => Self::Label,
            LabelType::ContactGroup => Self::ContactGroup,
            LabelType::Folder => Self::Folder,
            LabelType::System => Self::System,
        }
    }
}

impl FromSql for LabelType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Label),
            2 => Ok(Self::ContactGroup),
            3 => Ok(Self::Folder),
            4 => Ok(Self::System),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for LabelType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum MessageButtons {
    /// TODO: Document this variant.
    #[default]
    ReadFirst = 0,

    /// TODO: Document this variant.
    UnreadFirst = 1,
}

impl From<ApiMessageButtons> for MessageButtons {
    fn from(value: ApiMessageButtons) -> Self {
        match value {
            ApiMessageButtons::ReadFirst => Self::ReadFirst,
            ApiMessageButtons::UnreadFirst => Self::UnreadFirst,
        }
    }
}

impl FromSql for MessageButtons {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::ReadFirst),
            1 => Ok(Self::UnreadFirst),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for MessageButtons {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
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

impl From<ApiMimeType> for MimeType {
    fn from(value: ApiMimeType) -> Self {
        match value {
            ApiMimeType::ApplicationJson => Self::ApplicationJson,
            ApiMimeType::ApplicationPdf => Self::ApplicationPdf,
            ApiMimeType::MessageRfc822 => Self::MessageRfc822,
            ApiMimeType::MultipartMixed => Self::MultipartMixed,
            ApiMimeType::MultipartRelated => Self::MultipartRelated,
            ApiMimeType::TextHtml => Self::TextHtml,
            ApiMimeType::TextPlain => Self::TextPlain,
        }
    }
}

impl From<MimeType> for ApiMimeType {
    fn from(value: MimeType) -> Self {
        match value {
            MimeType::ApplicationJson => Self::ApplicationJson,
            MimeType::ApplicationPdf => Self::ApplicationPdf,
            MimeType::MessageRfc822 => Self::MessageRfc822,
            MimeType::MultipartMixed => Self::MultipartMixed,
            MimeType::MultipartRelated => Self::MultipartRelated,
            MimeType::TextHtml => Self::TextHtml,
            MimeType::TextPlain => Self::TextPlain,
        }
    }
}

impl From<MimeType> for CryptoMimeType {
    fn from(value: MimeType) -> Self {
        match value {
            MimeType::TextHtml => Self::Html,
            MimeType::TextPlain => Self::Text,
            _ => Self::Html,
        }
    }
}

impl FromSql for MimeType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::ApplicationJson),
            2 => Ok(Self::ApplicationPdf),
            3 => Ok(Self::MessageRfc822),
            4 => Ok(Self::MultipartMixed),
            5 => Ok(Self::MultipartRelated),
            6 => Ok(Self::TextHtml),
            7 => Ok(Self::TextPlain),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for MimeType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
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

impl From<ApiNextMessageOnMove> for NextMessageOnMove {
    fn from(value: ApiNextMessageOnMove) -> Self {
        match value {
            ApiNextMessageOnMove::DisabledExplicit => Self::DisabledExplicit,
            ApiNextMessageOnMove::DisabledImplicit => Self::DisabledImplicit,
            ApiNextMessageOnMove::EnabledExplicit => Self::EnabledExplicit,
        }
    }
}

impl FromSql for NextMessageOnMove {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::DisabledExplicit),
            1 => Ok(Self::DisabledImplicit),
            2 => Ok(Self::EnabledExplicit),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for NextMessageOnMove {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// A message parsed header value can either be a string or an array of strings.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ParsedHeaderValue {
    /// TODO: Document this variant.
    Array(Vec<String>),

    /// TODO: Document this variant.
    String(String),
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PgpScheme {
    /// TODO: Document this variant.
    Inline = 8,

    /// TODO: Document this variant.
    #[default]
    Mime = 16,
}

impl From<ApiPgpScheme> for PgpScheme {
    fn from(value: ApiPgpScheme) -> Self {
        match value {
            ApiPgpScheme::Inline => Self::Inline,
            ApiPgpScheme::Mime => Self::Mime,
        }
    }
}

impl From<PgpScheme> for CryptoPgpScheme {
    fn from(value: PgpScheme) -> Self {
        match value {
            PgpScheme::Inline => CryptoPgpScheme::PGPInline,
            PgpScheme::Mime => CryptoPgpScheme::PGPMime,
        }
    }
}

impl FromSql for PgpScheme {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            8 => Ok(Self::Inline),
            16 => Ok(Self::Mime),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for PgpScheme {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq)]
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

impl From<ApiPmSignature> for PmSignature {
    fn from(value: ApiPmSignature) -> Self {
        match value {
            ApiPmSignature::Disabled => Self::Disabled,
            ApiPmSignature::Enabled => Self::Enabled,
            ApiPmSignature::EnabledLocked => Self::EnabledLocked,
        }
    }
}

impl FromSql for PmSignature {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Disabled),
            1 => Ok(Self::Enabled),
            2 => Ok(Self::EnabledLocked),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for PmSignature {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
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

impl From<ApiShowImages> for ShowImages {
    fn from(value: ApiShowImages) -> Self {
        match value {
            ApiShowImages::DoNotAutoLoad => Self::DoNotAutoLoad,
            ApiShowImages::AutoLoadRemote => Self::AutoLoadRemote,
            ApiShowImages::AutoLoadEmbedded => Self::AutoLoadEmbedded,
            ApiShowImages::AutoLoadBoth => Self::AutoLoadBoth,
        }
    }
}

impl FromSql for ShowImages {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::DoNotAutoLoad),
            1 => Ok(Self::AutoLoadRemote),
            2 => Ok(Self::AutoLoadEmbedded),
            3 => Ok(Self::AutoLoadBoth),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ShowImages {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
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

impl From<ApiShowMoved> for ShowMoved {
    fn from(value: ApiShowMoved) -> Self {
        match value {
            ApiShowMoved::DoNotKeep => Self::DoNotKeep,
            ApiShowMoved::KeepInDrafts => Self::KeepInDrafts,
            ApiShowMoved::KeepInSent => Self::KeepInSent,
            ApiShowMoved::KeepBoth => Self::KeepBoth,
        }
    }
}

impl FromSql for ShowMoved {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::DoNotKeep),
            1 => Ok(Self::KeepInDrafts),
            2 => Ok(Self::KeepInSent),
            3 => Ok(Self::KeepBoth),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ShowMoved {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SpamAction {
    /// TODO: Document this variant.
    DoNothing = 0,

    /// TODO: Document this variant.
    UnsubscribeWithOneClick = 1,
}

impl From<ApiSpamAction> for SpamAction {
    fn from(value: ApiSpamAction) -> Self {
        match value {
            ApiSpamAction::DoNothing => Self::DoNothing,
            ApiSpamAction::UnsubscribeWithOneClick => Self::UnsubscribeWithOneClick,
        }
    }
}

impl FromSql for SpamAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::DoNothing),
            1 => Ok(Self::UnsubscribeWithOneClick),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for SpamAction {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
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

impl From<ApiSwipeAction> for SwipeAction {
    fn from(value: ApiSwipeAction) -> Self {
        match value {
            ApiSwipeAction::Trash => Self::Trash,
            ApiSwipeAction::Spam => Self::Spam,
            ApiSwipeAction::Star => Self::Star,
            ApiSwipeAction::Archive => Self::Archive,
            ApiSwipeAction::MarkAsRead => Self::MarkAsRead,
        }
    }
}

impl FromSql for SwipeAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Trash),
            1 => Ok(Self::Spam),
            2 => Ok(Self::Star),
            3 => Ok(Self::Archive),
            4 => Ok(Self::MarkAsRead),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for SwipeAction {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ViewLayout {
    /// TODO: Document this variant.
    #[default]
    Column = 0,

    /// TODO: Document this variant.
    Row = 1,
}

impl From<ApiViewLayout> for ViewLayout {
    fn from(value: ApiViewLayout) -> Self {
        match value {
            ApiViewLayout::Column => Self::Column,
            ApiViewLayout::Row => Self::Row,
        }
    }
}

impl FromSql for ViewLayout {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Column),
            1 => Ok(Self::Row),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ViewLayout {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ViewMode {
    /// TODO: Document this variant.
    #[default]
    Conversations = 0,

    /// TODO: Document this variant.
    Messages = 1,
}

impl From<ApiViewMode> for ViewMode {
    fn from(value: ApiViewMode) -> Self {
        match value {
            ApiViewMode::Conversations => Self::Conversations,
            ApiViewMode::Messages => Self::Messages,
        }
    }
}

impl FromSql for ViewMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Conversations),
            1 => Ok(Self::Messages),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ViewMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

//  STRUCTS
//==============================================================================

/// Wrapper type around [`RealAttachmentEncryptedSignature`] to implement
/// [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentEncryptedSignature {
    pub value: RealAttachmentEncryptedSignature,
}

impl Deref for AttachmentEncryptedSignature {
    type Target = RealAttachmentEncryptedSignature;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'de> Deserialize<'de> for AttachmentEncryptedSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(AttachmentEncryptedSignature {
            value: RealAttachmentEncryptedSignature::deserialize(deserializer)?,
        })
    }
}

impl From<AttachmentEncryptedSignature> for RealAttachmentEncryptedSignature {
    fn from(value: AttachmentEncryptedSignature) -> Self {
        value.value
    }
}

impl From<RealAttachmentEncryptedSignature> for AttachmentEncryptedSignature {
    fn from(value: RealAttachmentEncryptedSignature) -> Self {
        Self { value }
    }
}

impl Serialize for AttachmentEncryptedSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

sql_using_serde!(AttachmentEncryptedSignature);

/// Metadata for attachments.
///
/// The attachment metadata can come from 3 different places:
///
///   1. Inline attachment metadata on conversations/messages. This not complete
///      but is enough for clients to display basic information about the
///      attachments.
///
///   2. Attachment Metadata request. This is 98% complete and contains
///      everything except for some missing headers.
///
///   3. Get Message request. This includes 80% of the attachment data and the
///      attachment headers. Currently this is the only place where we will find
///      these headers.
///
/// The attachment data is all stored in one table and initialized partially
/// with data from all these sources.
///
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentMetadata {
    /// Local attachment id.
    pub local_id: Option<LocalAttachmentId>,

    /// Attachment Id on the server.
    pub remote_id: Option<AttachmentId>,

    /// Whether attachment is inlined or not.
    pub disposition: Disposition,

    /// Attachment mime type.
    pub mime_type: attachment::MimeType,

    /// Attachment file name.
    pub filename: String,

    /// Attachment size in bytes.
    pub size: u64,
}

impl From<ApiAttachmentMetadata> for AttachmentMetadata {
    fn from(value: ApiAttachmentMetadata) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            disposition: value.disposition.into(),
            mime_type: value.mime_type.parse().unwrap_or_default(),
            filename: value.name,
            size: value.size,
        }
    }
}

/// Wrapper type around [`RealAttachmentSignature`] to implement [`FromSql`] and
/// [`ToSql`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentSignature {
    pub value: RealAttachmentSignature,
}

impl Deref for AttachmentSignature {
    type Target = RealAttachmentSignature;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'de> Deserialize<'de> for AttachmentSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(AttachmentSignature {
            value: RealAttachmentSignature::deserialize(deserializer)?,
        })
    }
}

impl From<AttachmentSignature> for RealAttachmentSignature {
    fn from(value: AttachmentSignature) -> Self {
        value.value
    }
}

impl From<RealAttachmentSignature> for AttachmentSignature {
    fn from(value: RealAttachmentSignature) -> Self {
        Self { value }
    }
}

impl Serialize for AttachmentSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

sql_using_serde!(AttachmentSignature);

/// This struct is used to represent how many conversations there are linked to particular label
/// It's different than [`ConversationCounters`] by containing Remote Label ID instead of the local one.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationCount {
    /// Remote label ID
    pub label_id: LabelId,

    /// Total number of conversations linked to one label
    pub total: u64,

    /// Number of unread conversations linked to one label
    pub unread: u64,
}

impl From<ApiConversationCount> for ConversationCount {
    fn from(value: ApiConversationCount) -> Self {
        Self {
            label_id: value.label_id,
            total: value.total,
            unread: value.unread,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedMessageBody {
    /// TODO: Document this field.
    pub encrypted_body: String,

    /// TODO: Document this field.
    pub metadata: MessageBodyMetadata,
}

impl EncryptedMessageBody {
    /// Decrypt and convert the encrypted message into a [`DecryptedMessageBody`].
    ///
    /// # Errors
    ///
    /// Return error if the decryption failed.
    pub fn into_decrypted_message<P: PGPProviderSync>(
        self,
        ctx: Arc<MailUserContext>,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
    ) -> Result<DecryptedMessageBody, MessageError> {
        // TODO: Verify signature.
        let (decrypted_body, _) = self
            .decrypt(&pgp_provider, &address_keys)
            .inspect_err(|e| error!("Failed to decrypt message body: {e}"))?;

        match decrypted_body {
            DecryptedBody::Plain(body) => Ok(DecryptedMessageBody::new(
                body,
                self.metadata,
                None,
                None,
                ctx,
            )),
            DecryptedBody::Mime(ProcessedMessage {
                body,
                attachments,
                encrypted_subject,
                ..
            }) => Ok(DecryptedMessageBody::new(
                body,
                self.metadata,
                Some(attachments),
                encrypted_subject,
                ctx,
            )),
        }
    }
}

impl GettablePGPMessage for EncryptedMessageBody {
    /// Return the encrypted body of the message, this is a PGP message which
    /// may then go on to be decrypted
    fn pgp_message(&self) -> &[u8] {
        self.encrypted_body.as_bytes()
    }
}

impl DecryptableMessage for EncryptedMessageBody {
    /// TODO: Document this method.
    fn message_id(&self) -> Option<&str> {
        self.metadata.remote_message_id.as_ref().map(|v| v.as_ref())
    }

    /// TODO: Document this method.
    fn message_is_mime(&self) -> bool {
        self.metadata.mime_type == MimeType::MultipartMixed
    }
}

/// Wrapper type around [`RealKeyPackets`] to implement [`FromSql`] and
/// [`ToSql`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyPackets {
    pub value: RealKeyPackets,
}

impl Deref for KeyPackets {
    type Target = RealKeyPackets;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'de> Deserialize<'de> for KeyPackets {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(KeyPackets {
            value: RealKeyPackets::deserialize(deserializer)?,
        })
    }
}

impl From<KeyPackets> for RealKeyPackets {
    fn from(value: KeyPackets) -> Self {
        value.value
    }
}

impl From<RealKeyPackets> for KeyPackets {
    fn from(value: RealKeyPackets) -> Self {
        Self { value }
    }
}

impl Serialize for KeyPackets {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.serialize(serializer)
    }
}

sql_using_serde!(KeyPackets);

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct LabelColor(String);

impl LabelColor {
    pub fn purple() -> Self {
        Self("#8080FF".into())
    }

    pub fn black() -> Self {
        Self("#000000".into())
    }
}

impl Display for LabelColor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for LabelColor {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for LabelColor {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl FromSql for LabelColor {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().map(|s| LabelColor(s.to_string()))
    }
}

impl ToSql for LabelColor {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::from(self.0.clone()))
    }
}

/// Sender details of message
#[derive(Clone, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageSender {
    /// Recipient email address.
    // TODO: Proper email parsing
    pub address: String,

    /// TODO: Document this field.
    pub bimi_selector: Option<String>,

    /// Whether to display the sender image.
    pub display_sender_image: bool,

    /// Whether the address is a proton address.
    pub is_proton: bool,

    /// Whether address originated from simple login alias.
    pub is_simple_login: bool,

    /// Recipient display name.
    pub name: String,
}

impl MessageSender {
    /// Creates an AvatarInformation struct using the details of
    /// the first MessageAddress in the provided slice.
    ///
    pub fn avatar_info(address_list: &[MessageSender]) -> AvatarInformation {
        if let Some(address) = address_list.first() {
            AvatarInformation::from(address)
        } else {
            AvatarInformation::default()
        }
    }
}

impl From<MessageSender> for AvatarInformation {
    fn from(address: MessageSender) -> AvatarInformation {
        AvatarInformation::from(&address.name).or_else(&address.address)
    }
}

impl From<&MessageSender> for AvatarInformation {
    fn from(address: &MessageSender) -> AvatarInformation {
        AvatarInformation::from(&address.name).or_else(&address.address)
    }
}

impl From<ApiMessageSender> for MessageSender {
    fn from(value: ApiMessageSender) -> Self {
        Self {
            address: value.address,
            bimi_selector: value.bimi_selector,
            display_sender_image: value.display_sender_image,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            name: value.name,
        }
    }
}

impl From<&str> for MessageSender {
    fn from(value: &str) -> Self {
        Self {
            address: value.to_owned(),
            ..Default::default()
        }
    }
}

sql_using_serde!(MessageSender);

/// Recipient address information.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct MessageRecipient {
    /// Email of the recipient
    pub address: String,

    /// Whether the recipient is a proton address.
    pub is_proton: bool,

    /// Display name of the recipient, empty if none.
    pub name: String,

    /// Name of the address group this recipient belongs too.
    pub group: MaybeEmptyString,
}

impl MessageRecipient {
    /// Creates an AvatarInformation struct using the details of
    /// the first MessageAddress in the provided slice.
    ///
    pub fn avatar_info(recipients: &[MessageRecipient]) -> AvatarInformation {
        if let Some(recipient) = recipients.first() {
            AvatarInformation::from(recipient)
        } else {
            AvatarInformation::default()
        }
    }
}
impl From<ApiMessageRecipient> for MessageRecipient {
    fn from(value: ApiMessageRecipient) -> Self {
        Self {
            address: value.address,
            is_proton: value.is_proton,
            name: value.name,
            group: MaybeEmptyString::from_option(value.group),
        }
    }
}

impl From<MessageRecipient> for ApiMessageRecipient {
    fn from(value: MessageRecipient) -> Self {
        Self {
            address: value.address,
            is_proton: value.is_proton,
            name: value.name,
            group: value.group.into_option(),
        }
    }
}

impl From<MessageRecipient> for AvatarInformation {
    fn from(address: MessageRecipient) -> AvatarInformation {
        AvatarInformation::from(&address.name).or_else(&address.address)
    }
}

impl From<MessageSender> for MessageRecipient {
    fn from(value: MessageSender) -> MessageRecipient {
        Self {
            address: value.address,
            is_proton: value.is_proton,
            name: value.name,
            group: MaybeEmptyString(None),
        }
    }
}

impl From<&MessageRecipient> for AvatarInformation {
    fn from(address: &MessageRecipient) -> AvatarInformation {
        AvatarInformation::from(&address.name).or_else(&address.address)
    }
}

impl From<&str> for MessageRecipient {
    fn from(value: &str) -> Self {
        Self {
            address: value.to_owned(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageReplyTo {
    /// Email of the recipient
    pub address: String,

    /// Display name of the recipient,empty if none.
    pub name: String,
}

impl From<ApiMessageReplyTo> for MessageReplyTo {
    fn from(value: ApiMessageReplyTo) -> Self {
        Self {
            address: value.address,
            name: value.name,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageRecipients {
    pub value: Vec<MessageRecipient>,
}

impl From<Vec<MessageRecipient>> for MessageRecipients {
    fn from(value: Vec<MessageRecipient>) -> Self {
        Self { value }
    }
}

sql_using_serde!(MessageRecipients);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageSenders {
    pub value: Vec<MessageSender>,
}

sql_using_serde!(MessageSenders);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageReplyTos {
    pub value: Vec<MessageReplyTo>,
}

sql_using_serde!(MessageReplyTos);

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct MessageAttachment {
    /// TODO: Document this field.
    pub id: AttachmentId,

    /// TODO: Document this field.
    pub disposition: Disposition,

    /// TODO: Document this field.
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub headers: MessageAttachmentHeaders,

    /// TODO: Document this field.
    pub key_packets: KeyPackets,

    /// TODO: Document this field.
    // pub mime_type: String,
    pub mime_type: attachment::MimeType,

    /// TODO: Document this field.
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

impl From<ApiMessageAttachment> for MessageAttachment {
    fn from(value: ApiMessageAttachment) -> Self {
        Self {
            id: value.id,
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.map(|v| v.into()),
            headers: value.headers.into(),
            key_packets: value.key_packets.into(),
            mime_type: value.mime_type.parse().unwrap_or_default(),
            name: value.name,
            signature: value.signature.map(|v| v.into()),
            size: value.size,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl From<ApiMessageAttachmentHeaders> for MessageAttachmentHeaders {
    fn from(value: ApiMessageAttachmentHeaders) -> Self {
        Self {
            content_disposition: value.content_disposition,
            content_id: value.content_id,
            content_transfer_encoding: value.content_transfer_encoding,
            image_height: value.image_height,
            image_width: value.image_width,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageAttachmentInfo {
    /// TODO: Document this field.
    pub attachment: u32,

    /// TODO: Document this field.
    pub inline: u32,
}

impl From<ApiMessageAttachmentInfo> for MessageAttachmentInfo {
    fn from(value: ApiMessageAttachmentInfo) -> Self {
        Self {
            attachment: value.attachment,
            inline: value.inline,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
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

sql_using_serde!(MessageAttachmentInfos);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
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

sql_using_serde!(MessageAttachments);

/// This struct is used to represent how many messages there are linked to particular label
/// It's different than [`MessageCounters`] by containing Remote Label ID instead of the local one.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageCount {
    /// Remote label ID
    pub label_id: LabelId,

    /// Total number of messages linked to one label
    pub total: u64,

    /// Number of unread messages linked to one label
    pub unread: u64,
}

impl From<ApiMessageCount> for MessageCount {
    fn from(value: ApiMessageCount) -> Self {
        Self {
            label_id: value.label_id,
            total: value.total,
            unread: value.unread,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
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

impl MessageFlags {
    /// Check whether this message is an auto-sent reply.
    #[must_use]
    pub fn is_sent_auto(&self) -> bool {
        if !self.intersects(MessageFlags::SENT) {
            return false;
        }

        self.intersects(MessageFlags::AUTO)
    }

    /// Check whether this message is a draft.
    #[must_use]
    pub fn is_draft(&self) -> bool {
        !self.intersects(MessageFlags::SENT | MessageFlags::RECEIVED)
    }
}

impl From<ApiMessageFlags> for MessageFlags {
    fn from(value: ApiMessageFlags) -> Self {
        Self(value.bits())
    }
}

impl FromSql for MessageFlags {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        MessageFlags::from_bits(u64::column_result(value)?).ok_or(FromSqlError::InvalidType)
    }
}

impl ToSql for MessageFlags {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileSetting {
    /// TODO: Document this field.
    pub actions: Vec<String>,

    /// TODO: Document this field.
    pub is_custom: bool,
}

impl From<ApiMobileSetting> for MobileSetting {
    fn from(value: ApiMobileSetting) -> Self {
        Self {
            actions: value.actions,
            is_custom: value.is_custom,
        }
    }
}

/// All possible actions sent by API GET settings request
///
/// Found in MailSettings::MobileSettings::MessageToolbar::Actions /
///          MailSettings::MobileSettings::ConversationToolbar::Actions /
///          MailSettings::MobileSettings::ListToolbar::Actions
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MobileActions {
    Archive,
    Forward,
    Label,
    Move,
    Print,
    Remind,
    Reply,
    ReportPhishing,
    SaveAttachments,
    SavePDF,
    SenderEmails,
    Snooze,
    Spam,
    ToggleLight,
    ToggleRead,
    ToggleStar,
    Trash,
    ViewHeaders,
    ViewHTML,
}

impl MobileActions {
    /// Compute the actions to be seen in the bottom bar
    pub(crate) async fn bottom_bar_actions(
        tether: &Tether,
    ) -> Result<Vec<MobileActions>, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        if let Some(mobile_settings) = settings.mobile_settings {
            if mobile_settings.message_toolbar.is_custom {
                return mobile_settings
                    .message_toolbar
                    .actions
                    .iter()
                    .map(|a| MobileActions::from_str(a))
                    .collect::<Result<_, _>>();
            }
        } else {
            warn!("No mobile_settings defined in MailSettings");
        }
        Ok(vec![
            MobileActions::ToggleRead,
            MobileActions::Archive,
            MobileActions::Trash,
        ])
    }
}

impl FromStr for MobileActions {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, AppError> {
        match value {
            "archive" => Ok(Self::Archive),
            "forward" => Ok(Self::Forward),
            "label" => Ok(Self::Label),
            "move" => Ok(Self::Move),
            "print" => Ok(Self::Print),
            "remind" => Ok(Self::Remind),
            "reply" => Ok(Self::Reply),
            "report_phishing" => Ok(Self::ReportPhishing),
            "save_attachments" => Ok(Self::SaveAttachments),
            "save_pdf" => Ok(Self::SavePDF),
            "sender_emails" => Ok(Self::SenderEmails),
            "spam" => Ok(Self::Spam),
            "toggle_light" => Ok(Self::ToggleLight),
            "toggle_read" => Ok(Self::ToggleRead),
            "toggle_star" => Ok(Self::ToggleStar),
            "trash" => Ok(Self::Trash),
            "view_headers" => Ok(Self::ViewHeaders),
            "view_html" => Ok(Self::ViewHTML),
            s => Err(AppError::InvalidMobileActions(s.to_owned())),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MobileSettings {
    /// TODO: Document this field.
    pub conversation_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub list_toolbar: MobileSetting,

    /// TODO: Document this field.
    pub message_toolbar: MobileSetting,
}

impl From<ApiMobileSettings> for MobileSettings {
    fn from(value: ApiMobileSettings) -> Self {
        Self {
            conversation_toolbar: value.conversation_toolbar.into(),
            list_toolbar: value.list_toolbar.into(),
            message_toolbar: value.message_toolbar.into(),
        }
    }
}

sql_using_serde!(MobileSettings);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParsedHeaders {
    pub headers: HashMap<String, serde_json::Value>,
}

sql_using_serde!(ParsedHeaders);

//  TRAITS
//==============================================================================

/// System label identifiers that are constant for every account.
pub trait SystemLabelId: for<'a> From<&'a str> {
    #[must_use]
    fn all_drafts() -> Self {
        Self::from("1")
    }

    #[must_use]
    fn all_mail() -> Self {
        Self::from("5")
    }

    #[must_use]
    fn all_scheduled() -> Self {
        Self::from("12")
    }

    #[must_use]
    fn all_sent() -> Self {
        Self::from("2")
    }

    #[must_use]
    fn almost_all_mail() -> Self {
        Self::from("15")
    }

    #[must_use]
    fn archive() -> Self {
        Self::from("6")
    }

    #[must_use]
    fn drafts() -> Self {
        Self::from("8")
    }

    #[must_use]
    fn inbox() -> Self {
        Self::from("0")
    }

    #[must_use]
    fn movable_sys_folder_list() -> [Self; 4] {
        [Self::inbox(), Self::archive(), Self::spam(), Self::trash()]
    }
    #[must_use]
    fn outbox() -> Self {
        Self::from("9")
    }

    #[must_use]
    fn sent() -> Self {
        Self::from("7")
    }

    #[must_use]
    fn spam() -> Self {
        Self::from("4")
    }

    #[must_use]
    fn starred() -> Self {
        Self::from("10")
    }

    #[must_use]
    fn trash() -> Self {
        Self::from("3")
    }

    #[must_use]
    fn snoozed() -> Self {
        Self::from("16")
    }

    fn category_social() -> Self {
        Self::from("20")
    }

    fn category_promotions() -> Self {
        Self::from("21")
    }

    fn category_updates() -> Self {
        Self::from("22")
    }

    fn category_forums() -> Self {
        Self::from("23")
    }

    fn category_default() -> Self {
        Self::from("24")
    }
}

impl SystemLabelId for LabelId {}

/// Information about [`Label`] of type [`LabelType::Label`] that are applied
/// to [`Conversation`] or [`Messages`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CustomLabel {
    /// Local id of the label
    pub local_id: LocalLabelId,
    /// Name of the label
    pub name: String,
    /// Color of the label.
    pub color: LabelColor,
}

impl CustomLabel {
    /// Create a new instance from a `label`
    pub fn new(label: &Label) -> Self {
        Self {
            local_id: label.local_id.expect("Should be set"),
            name: label.name.clone(),
            color: label.color.clone(),
        }
    }
}

impl From<Label> for CustomLabel {
    fn from(value: Label) -> Self {
        Self {
            local_id: value.local_id.expect("Should be set"),
            name: value.name,
            color: value.color,
        }
    }
}

/// This enum is extended version of the `LabelType` enum. It contains additional
/// information regarding the system label type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LabelDescription {
    Label,
    ContactGroup,
    Folder,

    /// System field contain information about the system label type.
    /// SystemLabel main purpose is to determine the type of the system label.
    /// It is required for localization in the sidebar & dropdowns.
    /// The information is optional as we cannot for see all possible system labels.
    System(Option<SystemLabel>),
}

impl LabelDescription {
    #[must_use]
    pub fn new(label: &Label) -> Self {
        match label.label_type {
            LabelType::Label => LabelDescription::Label,
            LabelType::ContactGroup => LabelDescription::ContactGroup,
            LabelType::Folder => LabelDescription::Folder,
            LabelType::System => LabelDescription::System(SystemLabel::new(label)),
        }
    }
}

impl Display for LabelDescription {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System(_) => write!(f, "System"),
        }
    }
}

impl From<LabelDescription> for LabelType {
    fn from(value: LabelDescription) -> Self {
        match value {
            LabelDescription::Label => LabelType::Label,
            LabelDescription::ContactGroup => LabelType::ContactGroup,
            LabelDescription::Folder => LabelType::Folder,
            LabelDescription::System(_) => LabelType::System,
        }
    }
}

pub use proton_mail_ids::LocalAttachmentId;
pub use proton_mail_ids::LocalConversationId;
pub use proton_mail_ids::LocalMessageId;
