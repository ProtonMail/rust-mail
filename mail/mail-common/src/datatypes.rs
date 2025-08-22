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

mod assigned_actions;
pub mod attachment;
pub(crate) mod contextual_conversation;
pub mod dependencies;
pub mod exclusive_location;
pub mod folder_banner;
mod ids;
pub mod labels;
pub mod mail_notifications;
pub mod message_banner;
mod read_filter;
mod rollback_item_type;
mod search_options;
mod system_folder;
pub mod theme;

use stash::orm::Model;

pub use assigned_actions::*;
pub use contextual_conversation::*;
use derive_more::derive::TryFrom;
pub use exclusive_location::ExclusiveLocation;
pub use ids::*;
use indoc::formatdoc;
use proton_core_common::models::Label;
pub use read_filter::ReadFilter;
pub use rollback_item_type::RollbackItemType;
pub use search_options::SearchOptions;
use stash::stash::{Bond, StashError, Tether};
pub use system_folder::MovableSystemFolder;

use crate::actions::messages::UnsubscribeNewsletter;
use crate::decrypted_message::DecryptedMessageBody;
use crate::draft::recipients::MaybeEmptyString;
use crate::models::{
    Attachment, AttachmentType, MailSettings, MessageBodyMetadata, MessageMimeType,
};
use crate::{AppError, MailContextError, MailUserContext};
use attachment::{ContentId, MimeTypeCategory};
use core::fmt;
use proton_core_api::services::proton::{AddressId, LabelId, PrivateEmail, PrivateString};
use proton_core_common::datatypes::{
    AvatarInformation, LabelColor, LabelType, LocalLabelId, SystemLabel,
};
use proton_crypto_account::keys::{
    EmailMimeType as CryptoMimeType, PGPScheme as CryptoPgpScheme, UnlockedAddressKeys,
};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody, GettablePGPMessage};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_inbox_mime::{
    Disposition as CryptoDisposition, ProcessedBodyType, ProcessedMessage,
};
use proton_mail_api::services::proton::common::AttachmentId;
use proton_mail_api::services::proton::response_data::{
    AlmostAllMail as ApiAlmostAllMail, AttachmentMetadata as ApiAttachmentMetadata,
    ComposerDirection as ApiComposerDirection, ComposerMode as ApiComposerMode,
    ConversationCount as ApiConversationCount, Disposition as ApiDisposition,
    MessageAttachment as ApiMessageAttachment,
    MessageAttachmentHeaders as ApiMessageAttachmentHeaders,
    MessageAttachmentInfo as ApiMessageAttachmentInfo, MessageButtons as ApiMessageButtons,
    MessageCount as ApiMessageCount, MessageFlags as ApiMessageFlags,
    MessageRecipient as ApiMessageRecipient, MessageSender as ApiMessageSender,
    MimeType as ApiMimeType, MobileAction as ApiMobileAction, MobileSetting as ApiMobileSetting,
    MobileSettings as ApiMobileSettings, NextMessageOnMove as ApiNextMessageOnMove,
    PgpScheme as ApiPgpScheme, PmSignature as ApiPmSignature, ShowImages as ApiShowImages,
    ShowMoved as ApiShowMoved, SpamAction as ApiSpamAction, SwipeAction as ApiSwipeAction,
    ViewLayout as ApiViewLayout, ViewMode as ApiViewMode,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::{params, sql_using_serde};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use tracing::{error, trace};
//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for AlmostAllMail {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ComposerDirection {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ComposerMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// Whether this is an embedded attachment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, TryFrom, Default)]
#[try_from(repr)]
#[repr(u8)]
pub enum Disposition {
    #[default]
    /// This is meant to be shown as a regular attachment
    Attachment = 1,

    /// This is meant to be shown as an image inside of the message.
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

impl From<CryptoDisposition> for Disposition {
    fn from(value: CryptoDisposition) -> Self {
        match value {
            CryptoDisposition::Attachment => Self::Attachment,
            CryptoDisposition::Inline => Self::Inline,
        }
    }
}

impl FromSql for Disposition {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for Disposition {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for MessageButtons {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum MimeType {
    ApplicationJson = 1,
    ApplicationPdf = 2,
    MessageRfc822 = 3,
    MultipartMixed = 4,
    MultipartRelated = 5,
    #[default]
    TextHtml = 6,
    TextPlain = 7,
}

impl MimeType {
    pub fn supports_inline_attachments(&self) -> bool {
        matches!(
            self,
            Self::MultipartMixed | Self::MultipartRelated | Self::TextHtml
        )
    }
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for MimeType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
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
    String(String),
    Array(Vec<String>),
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for PgpScheme {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct PmSignature(u8);

bitflags::bitflags! {
    impl PmSignature:u8 {
        const ENABLED = 1 << 0;
        const LOCKED = 1 << 1;

        // Safeguard against unknown values
        const _ = !0;
    }
}

impl PmSignature {
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.intersects(PmSignature::ENABLED | PmSignature::LOCKED)
    }

    #[must_use]
    pub fn is_locked(self) -> bool {
        self.contains(Self::LOCKED)
    }

    #[must_use]
    pub fn is_unlocked(self) -> bool {
        !self.is_locked()
    }
}

impl From<ApiPmSignature> for PmSignature {
    fn from(value: ApiPmSignature) -> Self {
        Self(value.bits())
    }
}

impl FromSql for PmSignature {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        PmSignature::from_bits(val).ok_or(FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for PmSignature {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(self.bits() as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ShowImages {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ShowMoved {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for SpamAction {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// Where to move or what to do with the item when the user swipes it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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

impl From<ApiSwipeAction> for SwipeAction {
    fn from(value: ApiSwipeAction) -> Self {
        match value {
            ApiSwipeAction::NoAction => Self::NoAction,
            ApiSwipeAction::Trash => Self::Trash,
            ApiSwipeAction::Spam => Self::Spam,
            ApiSwipeAction::Star => Self::Star,
            ApiSwipeAction::Archive => Self::Archive,
            ApiSwipeAction::MarkAsRead => Self::MarkAsRead,
            ApiSwipeAction::MoveTo => Self::MoveTo,
            ApiSwipeAction::LabelAs => Self::LabelAs,
        }
    }
}

impl FromSql for SwipeAction {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = i8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for SwipeAction {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ViewLayout {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ViewMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// In certain label locations the message either needs to display the sender or the recipient.
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum MessageRecipientDisplayMode {
    Recipients,
    Sender,
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
    pub local_id: Option<LocalAttachmentId>,
    pub attachment_type: AttachmentType,
    pub disposition: Disposition,
    pub mime_type: attachment::MimeType,
    pub filename: String,
    pub size: u64,
}

impl AttachmentMetadata {
    pub fn remote_id(&self) -> Option<AttachmentId> {
        match &self.attachment_type {
            AttachmentType::Remote(id) => id.clone(),
            _ => None,
        }
    }

    /// Some attachments (e.g. GPG keys) are "not interesting" - they should be
    /// displayed when user _opens_ a message, but not as those small "pills" on
    /// the message/conversation list itself, as not to clutter the view.
    ///
    /// This function determines whether an attachment should be visible on the
    /// message/conversation list or not.
    pub fn is_listable(&self) -> bool {
        matches!(
            self.mime_type.category(),
            MimeTypeCategory::Audio
                | MimeTypeCategory::Excel
                | MimeTypeCategory::Image
                | MimeTypeCategory::Pdf
                | MimeTypeCategory::Powerpoint
                | MimeTypeCategory::Video
                | MimeTypeCategory::Word
        )
    }
}

impl From<ApiAttachmentMetadata> for AttachmentMetadata {
    fn from(value: ApiAttachmentMetadata) -> Self {
        Self {
            local_id: None,
            attachment_type: AttachmentType::Remote(Some(value.id)),
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
pub struct ConversationLabelsCount {
    /// Remote label ID
    pub label_id: LabelId,

    /// Total number of conversations linked to one label
    pub total: u64,

    /// Number of unread conversations linked to one label
    pub unread: u64,
}

impl From<ApiConversationCount> for ConversationLabelsCount {
    fn from(value: ApiConversationCount) -> Self {
        Self {
            label_id: value.label_id,
            total: value.total,
            unread: value.unread,
        }
    }
}

impl ConversationLabelsCount {
    pub async fn create_or_update_conversation_counts(
        counts: Vec<Self>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for count in counts {
            bond.execute(
                formatdoc!(
                    r"
                    INSERT INTO conversation_counters(local_label_id, total, unread)
                    SELECT l.local_id, ?, ?
                    FROM labels AS l
                    WHERE l.remote_id = ?
                    ON CONFLICT(local_label_id) DO UPDATE
                    SET total = ?,
                        unread = ?
                    "
                ),
                params![
                    count.total,
                    count.unread,
                    count.label_id,
                    count.total,
                    count.unread
                ],
            )
            .await?;
        }
        Ok(())
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
    /// It also stores the pgp attachments into the database.
    ///
    /// # Errors
    ///
    /// Return error if the decryption failed.
    pub async fn into_decrypted_message<P>(
        mut self,
        ctx: &MailUserContext,
        address_id: &AddressId,
        address_keys: UnlockedAddressKeys<P>,
        pgp: P,
        with_attachment_prefetch: bool,
    ) -> Result<DecryptedMessageBody, MailContextError>
    where
        P: PGPProviderSync,
    {
        let ctx = ctx
            .as_weak()
            .upgrade()
            .ok_or(MailContextError::MissingContext)?;

        match self.decrypt(&pgp, &address_keys) {
            Ok((decrypted_body, _)) => {
                let mime_type =
                    MessageMimeType::from_api(self.metadata.mime_type, || match &decrypted_body {
                        DecryptedBody::Plain(_) => unreachable!(),
                        DecryptedBody::Mime(msg) => match msg.mime_body_type {
                            ProcessedBodyType::Text => MessageMimeType::TextPlain,
                            ProcessedBodyType::Html | ProcessedBodyType::Empty => {
                                MessageMimeType::TextHtml
                            }
                        },
                    });

                // TODO: Verify signature.
                match decrypted_body {
                    DecryptedBody::Plain(body) => Ok(if with_attachment_prefetch {
                        DecryptedMessageBody::new_prefetching(
                            body,
                            self.metadata,
                            mime_type,
                            None,
                            address_id.clone(),
                            None,
                            ctx,
                        )
                    } else {
                        DecryptedMessageBody::new_without_prefetching(
                            body,
                            self.metadata,
                            mime_type,
                            None,
                            address_id.clone(),
                            None,
                        )
                    }),

                    DecryptedBody::Mime(ProcessedMessage {
                        body,
                        // We store the pgp attachments as normal attachments
                        attachments: pgp_attachments,
                        encrypted_subject,
                        ..
                    }) => {
                        tracing::info!(
                            "Message is PGP Encrypted with {} PGP attachment",
                            pgp_attachments.len()
                        );
                        // We create the models first to keep the tx open for less time.
                        let mut model_attachments = vec![];
                        for att in pgp_attachments {
                            let model_att = Attachment {
                                attachment_type: AttachmentType::Pgp,
                                content_id: Some(ContentId::from(att.content_id)),
                                disposition: att.disposition.into(),
                                filename: att.name,
                                size: att.size as u64,
                                mime_type: attachment::MimeType::from_str(&att.mime_type)
                                    .unwrap_or_default(),
                                local_message_id: self.metadata.local_message_id,
                                remote_message_id: self.metadata.remote_message_id.clone(),
                                ..Default::default()
                            };
                            model_attachments.push((model_att, att.data));
                        }

                        let mut tether = ctx.user_stash().connection();
                        tether
                            .tx::<_, _, MailContextError>(async |tx| {
                                for (mut att, data) in model_attachments {
                                    att.save(tx).await?;
                                    Attachment::store_in_cache(
                                        &ctx,
                                        &att.filename,
                                        att.id(),
                                        data,
                                        tx,
                                    )
                                    .await?;
                                    tracing::info!("Created PGP attachment {:?}", att.id());
                                    self.metadata.attachments.push(att);
                                }
                                Ok(self.metadata.save(tx).await?)
                            })
                            .await?;

                        Ok(if with_attachment_prefetch {
                            DecryptedMessageBody::new_prefetching(
                                body,
                                self.metadata,
                                mime_type,
                                encrypted_subject,
                                address_id.clone(),
                                None,
                                ctx,
                            )
                        } else {
                            DecryptedMessageBody::new_without_prefetching(
                                body,
                                self.metadata,
                                mime_type,
                                encrypted_subject,
                                address_id.clone(),
                                None,
                            )
                        })
                    }
                }
            }

            Err(e) => {
                error!(
                    "Failed to decrypt message body ({:?}): {e:?}",
                    self.metadata.remote_message_id,
                );

                // In the `Ok` code path we extract message's mime type from the
                // decrypted body - since in this case we've got no decrypted
                // body to work with, let's take an educated guess.
                //
                // This guess is going to be:
                //
                // - correct for non-mime-encrypted messages, since the mime
                //   type is then /not/ encrypted and whatever API told us is
                //   true,
                //
                // - incorrect for mime-encrypted messages - we'll default to
                //   text/plain then.
                //
                // Guessing incorrectly is not harmful, because users can't do
                // anything with non-decryptable messages anyway - i.e. the mime
                // type we use here is effectively left unread, it's just very
                // awkward to properly model this constraint in the type system.
                let mime_type = MessageMimeType::from_api(self.metadata.mime_type, || {
                    MessageMimeType::TextPlain
                });

                Ok(DecryptedMessageBody::not_decryptable(
                    self.encrypted_body,
                    self.metadata,
                    mime_type,
                    address_id.clone(),
                    e.to_string(),
                ))
            }
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

/// Sender details of message
#[derive(Clone, Default, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageSender {
    /// Recipient email address.
    // TODO: Proper email parsing
    pub address: PrivateEmail,

    /// TODO: Document this field.
    pub bimi_selector: Option<String>,

    /// Whether to display the sender image.
    pub display_sender_image: bool,

    /// Whether the address is a proton address.
    pub is_proton: bool,

    /// Whether address originated from simple login alias.
    pub is_simple_login: bool,

    /// Recipient display name.
    pub name: PrivateString,
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
        AvatarInformation::from(address.name.as_clear_text_str())
            .or_else(address.address.as_clear_text_str())
    }
}

impl From<&MessageSender> for AvatarInformation {
    fn from(address: &MessageSender) -> AvatarInformation {
        AvatarInformation::from(address.name.as_clear_text_str())
            .or_else(address.address.as_clear_text_str())
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
            address: value.into(),
            ..Default::default()
        }
    }
}

sql_using_serde!(MessageSender);

/// Recipient address information.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct MessageRecipient {
    /// Email of the recipient
    pub address: PrivateEmail,

    /// Whether the recipient is a proton address.
    pub is_proton: bool,

    /// Display name of the recipient, empty if none.
    pub name: PrivateString,

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
        AvatarInformation::from(address.name.as_clear_text_str())
            .or_else(address.address.as_clear_text_str())
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
        AvatarInformation::from(address.name.as_clear_text_str())
            .or_else(address.address.as_clear_text_str())
    }
}

impl From<&str> for MessageRecipient {
    fn from(value: &str) -> Self {
        Self {
            address: PrivateEmail::new(value),
            ..Default::default()
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

impl Deref for MessageRecipients {
    type Target = Vec<MessageRecipient>;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for MessageRecipients {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

sql_using_serde!(MessageRecipients);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MessageSenders {
    pub value: Vec<MessageSender>,
}

sql_using_serde!(MessageSenders);

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
pub struct MessageLabelsCount {
    /// Remote label ID
    pub label_id: LabelId,

    /// Total number of messages linked to one label
    pub total: u64,

    /// Number of unread messages linked to one label
    pub unread: u64,
}

impl From<ApiMessageCount> for MessageLabelsCount {
    fn from(value: ApiMessageCount) -> Self {
        Self {
            label_id: value.label_id,
            total: value.total,
            unread: value.unread,
        }
    }
}

impl MessageLabelsCount {
    pub async fn create_or_update_message_counts(
        counts: Vec<Self>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for count in counts {
            bond.execute(
                formatdoc!(
                    r"
                    INSERT INTO message_counters(local_label_id, total, unread)
                    SELECT l.local_id, ?, ?
                        FROM labels AS l
                        WHERE l.remote_id = ?
                    ON CONFLICT(local_label_id) DO UPDATE
                        SET total = ?,
                            unread = ?
                    "
                ),
                params![
                    count.total,
                    count.unread,
                    count.label_id,
                    count.total,
                    count.unread
                ],
            )
            .await?;
        }
        Ok(())
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

        /// Message is a reply to an Encrypted-Outside message
        const FLAG_EO_REPLY = 1 << 36;

        /// Snooze reminder should be displayed to the user
        const DISPLAY_SNOOZE_REMINDER = 1 << 37;

        // Safeguard against unknown values
        const _ = !0;
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

    #[must_use]
    pub fn is_schedule_send(&self) -> bool {
        self.intersects(MessageFlags::SCHEDULED_SEND)
    }

    #[must_use]
    pub fn is_sent(&self) -> bool {
        self.intersects(MessageFlags::SENT)
    }

    pub fn display_snooze_reminder(&self) -> bool {
        self.intersects(MessageFlags::DISPLAY_SNOOZE_REMINDER)
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
    pub actions: Vec<MobileAction>,

    /// TODO: Document this field.
    pub is_custom: bool,
}

impl From<ApiMobileSetting> for MobileSetting {
    fn from(value: ApiMobileSetting) -> Self {
        Self {
            actions: value.actions.into_iter().map(Into::into).collect(),
            is_custom: value.is_custom,
        }
    }
}

impl From<MobileSetting> for ApiMobileSetting {
    fn from(value: MobileSetting) -> Self {
        Self {
            actions: value.actions.into_iter().map(Into::into).collect(),
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
    #[serde(untagged)]
    Other(String),
}

impl From<ApiMobileAction> for MobileAction {
    fn from(value: ApiMobileAction) -> Self {
        match value {
            ApiMobileAction::Archive => Self::Archive,
            ApiMobileAction::Forward => Self::Forward,
            ApiMobileAction::Label => Self::Label,
            ApiMobileAction::Move => Self::Move,
            ApiMobileAction::Print => Self::Print,
            ApiMobileAction::Remind => Self::Remind,
            ApiMobileAction::Reply => Self::Reply,
            ApiMobileAction::ReportPhishing => Self::ReportPhishing,
            ApiMobileAction::SaveAttachments => Self::SaveAttachments,
            ApiMobileAction::SavePDF => Self::SavePDF,
            ApiMobileAction::SenderEmails => Self::SenderEmails,
            ApiMobileAction::Snooze => Self::Snooze,
            ApiMobileAction::Spam => Self::Spam,
            ApiMobileAction::ToggleLight => Self::ToggleLight,
            ApiMobileAction::ToggleRead => Self::ToggleRead,
            ApiMobileAction::ToggleStar => Self::ToggleStar,
            ApiMobileAction::Trash => Self::Trash,
            ApiMobileAction::ViewHeaders => Self::ViewHeaders,
            ApiMobileAction::ViewHTML => Self::ViewHTML,
            ApiMobileAction::Other(s) => Self::Other(s),
        }
    }
}

impl From<MobileAction> for ApiMobileAction {
    fn from(value: MobileAction) -> Self {
        match value {
            MobileAction::Archive => Self::Archive,
            MobileAction::Forward => Self::Forward,
            MobileAction::Label => Self::Label,
            MobileAction::Move => Self::Move,
            MobileAction::Print => Self::Print,
            MobileAction::Remind => Self::Remind,
            MobileAction::Reply => Self::Reply,
            MobileAction::ReportPhishing => Self::ReportPhishing,
            MobileAction::SaveAttachments => Self::SaveAttachments,
            MobileAction::SavePDF => Self::SavePDF,
            MobileAction::SenderEmails => Self::SenderEmails,
            MobileAction::Snooze => Self::Snooze,
            MobileAction::Spam => Self::Spam,
            MobileAction::ToggleLight => Self::ToggleLight,
            MobileAction::ToggleRead => Self::ToggleRead,
            MobileAction::ToggleStar => Self::ToggleStar,
            MobileAction::Trash => Self::Trash,
            MobileAction::ViewHeaders => Self::ViewHeaders,
            MobileAction::ViewHTML => Self::ViewHTML,
            MobileAction::Other(s) => Self::Other(s),
        }
    }
}

impl MobileAction {
    /// Compute the actions seen in the bottom bar on the list view (regardless of conversation grouping)
    pub async fn list_toolbar_actions(tether: &Tether) -> Result<Vec<MobileAction>, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        let actions = match settings.mobile_settings {
            Some(mobile_settings) => {
                Self::toolbar_actions_from_setting(&mobile_settings.list_toolbar)
            }
            None => {
                trace!("No mobile_settings defined in MailSettings");
                Self::default_chosen_actions()
            }
        };

        Ok(actions)
    }

    /// Compute the actions seen in the bottom bar and action sheet for conversation view
    pub async fn conversation_toolbar_actions(
        tether: &Tether,
    ) -> Result<Vec<MobileAction>, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        let actions = match settings.mobile_settings {
            Some(mobile_settings) => {
                Self::toolbar_actions_from_setting(&mobile_settings.conversation_toolbar)
            }
            None => {
                trace!("No mobile_settings defined in MailSettings");
                Self::default_chosen_actions()
            }
        };

        Ok(actions)
    }

    /// Compute the actions seen in the bottom bar and action sheet for message view
    pub async fn message_toolbar_actions(tether: &Tether) -> Result<Vec<MobileAction>, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        let actions = match settings.mobile_settings {
            Some(mobile_settings) => {
                Self::toolbar_actions_from_setting(&mobile_settings.message_toolbar)
            }
            None => {
                trace!("No mobile_settings defined in MailSettings");
                Self::default_chosen_actions()
            }
        };

        Ok(actions)
    }

    pub fn default_chosen_actions() -> Vec<MobileAction> {
        use self::MobileAction::*;
        vec![ToggleRead, Trash, Move]
    }

    pub fn all_list_actions() -> Vec<MobileAction> {
        use self::MobileAction::*;
        vec![
            ToggleRead, Trash, Move, Label, ToggleStar, Snooze, Archive, Spam,
        ]
    }

    pub fn all_conversation_actions() -> Vec<MobileAction> {
        use self::MobileAction::*;
        vec![
            ToggleRead, Trash, Move, Label, ToggleStar, Snooze, Archive, Spam,
        ]
    }

    pub fn all_message_actions() -> Vec<MobileAction> {
        use self::MobileAction::*;
        vec![
            ToggleRead,
            Trash,
            Move,
            Label,
            ToggleStar,
            Archive,
            Spam,
            Reply,
            Forward,
            SavePDF,
            Print,
            ReportPhishing,
            ViewHeaders,
            ViewHTML,
        ]
    }

    fn toolbar_actions_from_setting(mobile_setting: &MobileSetting) -> Vec<MobileAction> {
        if mobile_setting.is_custom {
            mobile_setting.actions.clone()
        } else {
            Self::default_chosen_actions()
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

impl From<MobileSettings> for ApiMobileSettings {
    fn from(value: MobileSettings) -> Self {
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

impl ParsedHeaders {
    pub fn can_unsubscribe(&self) -> bool {
        // TODO: implement unsubscribe via email
        // self.headers.get("List-Unsubscribe").is_some()
        // HACK: This is only necessary until it's implemented.
        UnsubscribeNewsletter::new(self, LocalMessageId::from(0)).is_some()
    }
}

/// An error during SQL deserialization.
/// It means we expected [`MAGIC_ID`] but got {0}
#[derive(Debug, thiserror::Error)]
#[error("Expected constant {expected} local id but got {got}")]
pub struct NotAMagicLocalIdError {
    pub expected: u32,
    pub got: u32,
}

/// Mail settings local id. This is a special value that ALWAYS must be equal the constant
/// [`MAGIC_ID`]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct MailSettingsId;

impl MailSettingsId {
    const MAGIC_ID: u32 = 1;
}

impl FromSql for MailSettingsId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let got = u32::from(u8::column_result(value)?);
        if got != Self::MAGIC_ID {
            return Err(FromSqlError::Other(Box::new(NotAMagicLocalIdError {
                expected: Self::MAGIC_ID,
                got,
            })));
        }
        Ok(Self)
    }
}

impl ToSql for MailSettingsId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(i64::from(
            Self::MAGIC_ID,
        ))))
    }
}

//  TRAITS
//==============================================================================

/// System label identifiers that are constant for every account.
pub trait SystemLabelId: for<'a> From<&'a str> {
    #[must_use]
    fn inbox() -> Self {
        Self::from("0")
    }

    #[must_use]
    fn all_drafts() -> Self {
        Self::from("1")
    }

    #[must_use]
    fn all_sent() -> Self {
        Self::from("2")
    }

    #[must_use]
    fn trash() -> Self {
        Self::from("3")
    }

    #[must_use]
    fn spam() -> Self {
        Self::from("4")
    }

    #[must_use]
    fn all_mail() -> Self {
        Self::from("5")
    }

    #[must_use]
    fn archive() -> Self {
        Self::from("6")
    }

    #[must_use]
    fn sent() -> Self {
        Self::from("7")
    }

    #[must_use]
    fn drafts() -> Self {
        Self::from("8")
    }

    #[must_use]
    fn outbox() -> Self {
        Self::from("9")
    }

    #[must_use]
    fn starred() -> Self {
        Self::from("10")
    }

    #[must_use]
    fn all_scheduled() -> Self {
        Self::from("12")
    }

    #[must_use]
    fn blocked() -> Self {
        Self::from("14")
    }

    #[must_use]
    fn almost_all_mail() -> Self {
        Self::from("15")
    }

    #[must_use]
    fn snoozed() -> Self {
        Self::from("16")
    }

    #[must_use]
    fn pinned() -> Self {
        Self::from("17")
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

    #[must_use]
    fn movable_sys_folder_list() -> [Self; 4] {
        [Self::inbox(), Self::archive(), Self::spam(), Self::trash()]
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
            local_id: label.id(),
            name: label.name.clone(),
            color: label.color.clone(),
        }
    }
}

impl From<Label> for CustomLabel {
    fn from(value: Label) -> Self {
        Self {
            local_id: value.id(),
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

pub use crate::datatypes::LocalAttachmentId;
pub use crate::datatypes::LocalConversationId;
pub use crate::datatypes::LocalMessageId;
