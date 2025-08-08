use crate::MailContextError;
use crate::datatypes::attachment::ContentId;
use crate::datatypes::{LocalAttachmentId, LocalMessageId};
use crate::models::MetadataId;
use chrono::{DateTime, Local};
use derive_more::derive::TryFrom;
use proton_account_api::ApiError;
use proton_action_queue::action::ActionId;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{AddressId, PrivateEmail};
use proton_core_common::datatypes::{LocalAddressId, UnixTimestamp};
use proton_crypto_inbox::attachment::{AttachmentDecryptionError, AttachmentEncryptionError};
use proton_crypto_inbox::eo::EoError;
use proton_crypto_inbox::keys::{PackageCryptoType, SessionKeyError};
use proton_crypto_inbox::message::MessageError;
use proton_mail_api::services::proton::request_data::DraftAction;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, ToSql, ToSqlOutput};
use tracing::error;

pub mod attachments;
pub mod compose;
mod draft_v1;
pub mod observers;
pub mod recipients;
pub(crate) mod send;

pub use crate::draft::send::EoData;
pub use send::ScheduleSendOptions;

pub use draft_v1::*;

pub const MIN_PASSWORD_LEN: usize = 8;
pub const MIN_EXPIRATION_TIME_SECONDS: u64 = 15 * 60; // 15 min

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    Send(#[from] SendError),
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Discard(#[from] DiscardError),
    #[error(transparent)]
    Undo(#[from] UndoError),
    #[error(transparent)]
    AttachmentUpload(#[from] AttachmentUploadError),
    #[error(transparent)]
    AttachmentRemove(#[from] AttachmentRemoveError),
    #[error(transparent)]
    CancelScheduleSend(#[from] CancelScheduleSendError),
    #[error(transparent)]
    SenderAddressChange(#[from] SenderAddressChangeError),
    #[error(transparent)]
    Password(PasswordError),
    #[error(transparent)]
    Expiration(ExpirationError),
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("No addresses found for current user")]
    UserHasNoAddresses,
    #[error("User Address {0} not found")]
    AddressNotFound(AddressId),
    #[error("Message {0} is not a draft")]
    MessageNotADraft(LocalMessageId),
    #[error("Message Body for {0} missing")]
    MessageBodyMissing(LocalMessageId),
    #[error("Can't reply or forward to a draft message {0}")]
    ReplyOrForwardToDraft(LocalMessageId),
}

impl From<OpenError> for MailContextError {
    fn from(err: OpenError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("Message {0} is not a draft")]
    MessageIsNotADraft(LocalMessageId),
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Draft has no message")]
    LocalDraftWithoutMessage,
    #[error("Draft send failed: {0}")]
    SendMessage(#[from] PackageError),
    #[error("Draft has no recipients")]
    NoRecipients,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
    #[error("Draft has attachments which have not uploaded")]
    MissingAttachmentUploads,
    #[error("Message Body for {0} missing")]
    MessageBodyMissing(LocalMessageId),
    #[error("Unable to schedule send before expected delivery time")]
    ScheduleSendExpired,
    #[error("The maximum amount of scheduled messages has been reached")]
    ScheduleSendMessageLimitExceeded,
    #[error("Failed to decrypt external encryption password")]
    EOPasswordDecrypt,
    #[error("Expiration time was too soon")]
    ExpirationTimeTooSoon,
}

impl From<SendError> for MailContextError {
    fn from(err: SendError) -> Self {
        Self::Draft(err.into())
    }
}

/// Errors that occur when saving a draft.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("No addresses found for current user")]
    UserHasNoAddresses,
    #[error("User Address {0} not found")]
    AddressNotFound(AddressId),
    #[error("User Address {0} has no primary key")]
    AddressWithoutPrimaryKey(AddressId),
    #[error("Message {0} is not a draft")]
    MessageNotADraft(LocalMessageId),
    #[error("Message Body for {0} missing")]
    MessageBodyMissing(LocalMessageId),
    #[error("Attachment {0} does not have key packets")]
    AttachmentDoesNotHaveKeyPackets(LocalAttachmentId),
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Draft has no message")]
    LocalDraftWithoutMessage,
    #[error("Can not update a draft that was sent")]
    AlreadySent,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
}

impl From<SaveError> for MailContextError {
    fn from(err: SaveError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentUploadError {
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Attachment Metadata for Attachment {0} does not exist")]
    AttachmentMetadataNotFound(LocalAttachmentId),
    #[error("Attachment Metadata for Attachment with content_id {0} does not exist")]
    AttachmentMetadataNotFoundCid(ContentId),
    #[error("Draft has no message")]
    MessageDoesNotExist,
    #[error("Attachment's can't be uploaded because message {0} does not exist on server")]
    MessageDoesNotExistOnServer(LocalMessageId),
    #[error("Attachment {0} is missing from the cache")]
    AttachmentDataMissing(LocalAttachmentId),
    #[error("Attachment {0} has inline disposition, but does not have a content id")]
    MissingContentId(LocalAttachmentId),
    #[error("Failed to encrypt attachment: {0}")]
    Crypto(AttachmentEncryptionError),
    #[error("An existing upload action exists for this attachment")]
    ExistingUploadActionExist(ActionId),
    #[error("Attachment has already been uploaded")]
    AttachmentAlreadyUploaded(LocalAttachmentId),
    #[error("The message has too many attachments")]
    TooManyAttachments,
    #[error("The message has already been sent")]
    MessageAlreadySent,
    #[error("Attachment size is greater than maximum limit")]
    AttachmentTooLarge,
    #[error("Retry attempted for Attachment {0} when not in error state")]
    RetryInvalidState(LocalAttachmentId),
    #[error("Combined attachment size is greater than maximum limit")]
    TotalAttachmentSizeTooLarge,
}

impl From<AttachmentUploadError> for MailContextError {
    fn from(err: AttachmentUploadError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentRemoveError {
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Attachment Metadata for Attachment {0} does not exist")]
    AttachmentMetadataNotFound(LocalAttachmentId),
}

impl From<AttachmentRemoveError> for MailContextError {
    fn from(err: AttachmentRemoveError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    #[error("Message {0} is not a draft")]
    MessageNotADraft(LocalMessageId),
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Can not undo send message {0}")]
    MessageCanNotBeUndoSent(LocalMessageId),
    #[error("Can no longer undo send for message")]
    SendCanNoLongerBeUndone,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
}

impl From<UndoError> for MailContextError {
    fn from(err: UndoError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscardError {
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Failed to delete draft on server")]
    DeleteFailed,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
}

impl From<DiscardError> for MailContextError {
    fn from(err: DiscardError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CancelScheduleSendError {
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Message with Id {0} does not exist")]
    MessageNotFound(LocalMessageId),
    #[error("Message {0} is not scheduled for sending")]
    MessageIsNotScheduled(LocalMessageId),
    #[error("Timed out while waiting on schedule send to complete")]
    TimedOut,
    #[error("Message {0} was already sent and can no longer be cancelled")]
    AlreadySent(LocalMessageId),
}

impl From<CancelScheduleSendError> for MailContextError {
    fn from(err: CancelScheduleSendError) -> Self {
        Self::Draft(err.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("Failed to encrypt package: {0}")]
    PackageBodyEncrypt(#[from] MessageError),
    #[error("Failed to load attachment content for mime body: {0}")]
    MimeBodyAttachmentLoad(#[from] ApiServiceError),
    #[error("Attachment Data Missing")]
    AttachmentDataMissing,
    #[error("Attachment failed to load: {0}")]
    AttachmentLoad(Box<MailContextError>),
    #[error("Attachment has no local id")]
    AttachmentHasNoLocalId,
    #[error("Attachment has no remote id")]
    AttachmentHasNoRemoteId,
    #[error("Attachment already has remote id")]
    AttachmentAlreadyHasRemoteId,
    #[error("Failed to get attachment address key {0}")]
    AttachmentAddressKeyMissing(AddressId),
    #[error("Failed to write mime body to buffer: {0}")]
    MimeBodyBuild(String),
    #[error("Failed to extract attachment info for address: {0}")]
    PackageBodyInfoReEncrypt(SessionKeyError),
    #[error("Failed to extract attachment info for address: {0}")]
    PackageAttachmentInfo(#[from] AttachmentDecryptionError),
    #[error("Failed to build package for encrypt-to-outside (EO): {0}")]
    PackageEo(#[from] EoError),
    #[error("EO selected but no password found")]
    PackageEoPasswordMissing,
    #[error("Failed to encrypt attachment info to recipient: {0}")]
    PackageAttachmentInfoReEncrypt(SessionKeyError),
    #[error("Failed to encrypt attachment signature to recipient: {0}")]
    PackageAttachmentInfoReEncryptSignature(AttachmentEncryptionError),
    #[error("Package encryption type is is not supported: {0}")]
    NotSupported(PackageCryptoType),
    #[error("Should encrypt but no recipient key found")]
    NoRecipientKey,
    #[error("Primary key not found")]
    PrimaryKeyNotFound,
    #[error("Invalid Recipient Email: {0}")]
    RecipientEmailInvalid(PrivateEmail),
    #[error("Proton Email {0} does not exist")]
    ProtonRecipientDoesNotExist(PrivateEmail),
    #[error("Modulus: {0}")]
    ModulusRequest(ApiError),
}

#[derive(Debug, thiserror::Error)]
pub enum SenderAddressChangeError {
    #[error("Draft metadata {0} not found")]
    MetadataNotFound(MetadataId),
    #[error("Address with email '{0}' not found")]
    AddressEmailNotFound(String),
    #[error("Address with id {0} not found")]
    AddressNotFound(AddressId),
    #[error("Can not send from address {0}")]
    AddressNotSendEnabled(AddressId),
    #[error("Address {0} is disabled")]
    AddressDisabled(AddressId),
    #[error("Address '{0}' has no remote id")]
    AddressHasNoRemoteId(LocalAddressId),
}

impl From<SenderAddressChangeError> for MailContextError {
    fn from(value: SenderAddressChangeError) -> Self {
        MailContextError::Draft(Error::SenderAddressChange(value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("Draft metadata {0} not found")]
    MetadataNotFound(MetadataId),
    #[error("Password should be at least 8 chars")]
    PasswordTooShort,
    #[error("Failed to encrypt password")]
    Encryption,
    #[error("Failed to decrypt password")]
    Decryption,
}

impl From<PasswordError> for MailContextError {
    fn from(err: PasswordError) -> Self {
        MailContextError::Draft(Error::Password(err))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExpirationError {
    #[error("Draft metadata {0} not found")]
    MetadataNotFound(MetadataId),
    #[error("Expiration time is older than the current time")]
    ExpirationTimeInThePast,
    #[error("Expiration time should be greater or egual to 15 min")]
    ExpirationTimeLessThan15Min,
    #[error("Expiration time exceeded 28 days")]
    ExpirationTimeExceeds28Days,
}

impl From<ExpirationError> for MailContextError {
    fn from(err: ExpirationError) -> Self {
        MailContextError::Draft(Error::Expiration(err))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum ReplyMode {
    Sender = 0,
    All = 1,
    Forward = 2,
}

impl ToSql for ReplyMode {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

impl FromSql for ReplyMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl From<ReplyMode> for DraftAction {
    fn from(value: ReplyMode) -> Self {
        match value {
            ReplyMode::Sender => DraftAction::Reply,
            ReplyMode::All => DraftAction::ReplyAll,
            ReplyMode::Forward => DraftAction::Forward,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DraftExpirationTime {
    Never,
    OneHour,
    OneDay,
    ThreeDays,
    Custom(DateTime<Local>),
}

impl DraftExpirationTime {
    pub fn to_timestamp(self) -> UnixTimestamp {
        match self {
            DraftExpirationTime::Never => UnixTimestamp::new(0),
            DraftExpirationTime::OneHour => UnixTimestamp::now().saturating_add(3600),
            DraftExpirationTime::OneDay => UnixTimestamp::now().saturating_add(86400), // 1 day
            DraftExpirationTime::ThreeDays => UnixTimestamp::now().saturating_add(86400 * 3),
            DraftExpirationTime::Custom(v) => v.into(),
        }
    }

    pub fn to_optional_timestamp(self) -> Option<UnixTimestamp> {
        match self {
            DraftExpirationTime::Never => None,
            _ => Some(self.to_timestamp()),
        }
    }
}
