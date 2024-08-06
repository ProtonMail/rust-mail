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

pub(crate) mod exclusive_location;

use crate::models::MessageBodyMetadata;
use core::fmt;
pub use exclusive_location::ExclusiveLocation;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::{
    AlmostAllMail as ApiAlmostAllMail, AttachmentMetadata as ApiAttachmentMetadata,
    ComposerDirection as ApiComposerDirection, ComposerMode as ApiComposerMode,
    ConversationCount as ApiConversationCount, Disposition as ApiDisposition,
    MessageAddress as ApiMessageAddress, MessageAttachment as ApiMessageAttachment,
    MessageAttachmentHeaders as ApiMessageAttachmentHeaders,
    MessageAttachmentInfo as ApiMessageAttachmentInfo, MessageButtons as ApiMessageButtons,
    MessageCount as ApiMessageCount, MessageFlags as ApiMessageFlags, MimeType as ApiMimeType,
    MobileSetting as ApiMobileSetting, MobileSettings as ApiMobileSettings,
    NextMessageOnMove as ApiNextMessageOnMove, PgpScheme as ApiPgpScheme,
    PmSignature as ApiPmSignature, ShowImages as ApiShowImages, ShowMoved as ApiShowMoved,
    SpamAction as ApiSpamAction, SwipeAction as ApiSwipeAction, ViewLayout as ApiViewLayout,
    ViewMode as ApiViewMode,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{DecryptableMessage, GettablePGPMessage};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::sql_using_serde;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use tracing::warn;

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

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl From<ApiAttachmentMetadata> for AttachmentMetadata {
    fn from(value: ApiAttachmentMetadata) -> Self {
        Self {
            remote_id: Some(value.id.into()),
            disposition: value.disposition.into(),
            mime_type: value.mime_type.into(),
            name: value.name,
            size: value.size,
        }
    }
}

sql_using_serde!(AttachmentMetadata);

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
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AttachmentMetadatas {
    pub value: Vec<AttachmentMetadata>,
}

sql_using_serde!(AttachmentMetadatas);

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

/// TODO: Document this struct.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationCount {
    /// TODO: Document this field.
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

impl From<ApiConversationCount> for ConversationCount {
    fn from(value: ApiConversationCount) -> Self {
        Self {
            label_id: value.label_id.into(),
            total: value.total,
            unread: value.unread,
        }
    }
}

/// Consists of the message's body metadata and decrypted content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecryptedMessageBody {
    /// The decrypted message contents.
    pub body: String,

    /// Metadata associated with the message body
    pub metadata: MessageBodyMetadata,
}

impl DecryptedMessageBody {
    /// Retrieve a parsed header value for a given `key`.
    /// TODO: Properly document this method.
    ///
    /// # Parameters
    ///
    /// * `key` - The key to retrieve the header value for.
    ///
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        let value = self
            .metadata
            .parsed_headers
            .headers
            .get(key)
            .and_then(|json_str| serde_json::from_str(json_str).ok());
        match value {
            Some(JsonValue::String(s)) => Some(ParsedHeaderValue::String(s.clone())),
            Some(JsonValue::Array(array)) => {
                let mut result = Vec::with_capacity(array.len());
                for (idx, item) in array.iter().enumerate() {
                    if let JsonValue::String(str) = item {
                        result.push(str.clone());
                    } else {
                        warn!(
                            "Header array value {key}[{idx}] of message {:?} has invalid value type",
                            self.metadata.local_message_id
                        );
                    }
                }
                Some(ParsedHeaderValue::Array(result))
            }
            _ => {
                warn!(
                    "Header value {key} of message {:?} has invalid value type",
                    self.metadata.local_message_id
                );
                None
            }
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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

/// TODO: Document this struct.
#[derive(Clone, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl From<ApiMessageAddress> for MessageAddress {
    fn from(value: ApiMessageAddress) -> Self {
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

sql_using_serde!(MessageAddress);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageAddresses {
    pub value: Vec<MessageAddress>,
}

sql_using_serde!(MessageAddresses);

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
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

impl From<ApiMessageAttachment> for MessageAttachment {
    fn from(value: ApiMessageAttachment) -> Self {
        Self {
            id: value.id.into(),
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.map(|v| v.into()),
            headers: value.headers.into(),
            key_packets: value.key_packets.into(),
            mime_type: value.mime_type.into(),
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

/// TODO: Document this struct.
// TODO: This does not get saved directly in the database, so perhaps could be
// TODO: removed from here and the API type used directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageCount {
    /// TODO: Document this field.
    pub label_id: LabelId,

    /// TODO: Document this field.
    pub total: u64,

    /// TODO: Document this field.
    pub unread: u64,
}

/// TODO: Document this struct.
impl From<ApiMessageCount> for MessageCount {
    fn from(value: ApiMessageCount) -> Self {
        Self {
            label_id: value.label_id.into(),
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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    pub headers: HashMap<String, String>,
}

sql_using_serde!(ParsedHeaders);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RemoteIds {
    pub value: Vec<RemoteId>,
}

sql_using_serde!(RemoteIds);

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
}

impl SystemLabelId for LabelId {}
