use crate::datatypes::attachment::{CombinedAttachmentDisposition, ContentId};
use crate::datatypes::{Disposition, LocalAttachmentId, LocalConversationId, LocalMessageId};
use crate::ios_share_ext::IosShareExtension;
use crate::models::{
    Attachment, AttachmentData, DraftMetadata, DraftSendResult, MailSettings, MessageMimeType,
    MetadataId,
};
use crate::{ImagePolicy, MailContextError, MailContextResult, MailUserContext};
use chrono::{DateTime, Local};
use derive_more::Display;
use derive_more::derive::TryFrom;
use non_empty_string::NonEmptyString;
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
use proton_mail_api::services::proton::response_data::Message as ApiMessage;
use proton_mailto::Mailto;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, ToSql, ToSqlOutput};
use std::fmt;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Weak;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

pub mod attachments;
pub mod compose;
pub mod draft_v1;
pub mod observers;
pub mod recipients;
pub(crate) mod send;

pub use crate::draft::send::EoData;
pub use send::ScheduleSendOptions;

use crate::actions::draft;
use crate::actions::draft::{Discard, Save, UndoSend};
use crate::decrypted_message::ThemeOpts;
use crate::draft::attachments::DraftAttachment;
use crate::draft::compose::DraftAddressValidationResult;
use crate::draft::recipients::{
    ExpirationFeatureSupportReport, OnBackgroundValidationComplete, Recipient, RecipientEntry,
    RecipientError, RecipientList, RecipientValidationUpdate, ValidatingRecipientList,
};
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput};
use proton_core_api::session::Session;
use proton_core_common::Origin;
use proton_core_common::models::Address;
use proton_mail_api::services::proton::common::{AttachmentId, MessageId};
use proton_mail_api::services::proton::prelude::DraftReplyOrForwardParams;
use stash::stash::Tether;

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
    #[error(transparent)]
    Recipient(#[from] RecipientError),
    #[error(transparent)]
    AttachmentDispositionSwap(#[from] AttachmentDispositionSwapError),
    #[error("Failed to communicate with actor")]
    Actor,
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
    #[error("Missing share extension's stub-draft")]
    ShareExtensionStubDraftMissing,
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
    #[error("Combined Message and Attachment size too large")]
    MessageTooLarge,
}

impl From<SendError> for MailContextError {
    fn from(err: SendError) -> Self {
        Self::Draft(err.into())
    }
}

/// Errors that occur when saving a draft.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("User Address {0} not found")]
    AddressNotFound(AddressId),
    #[error("User Address {0} has no primary key")]
    AddressWithoutPrimaryKey(AddressId),
    #[error("Message {0} is not a draft")]
    MessageNotADraft(LocalMessageId),
    #[error("Attachment {0} does not have key packets")]
    AttachmentDoesNotHaveKeyPackets(LocalAttachmentId),
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Can not update a draft that was sent")]
    AlreadySent,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
    #[error("Metadata missing local conversation id")]
    MetadataMissingLocalConversationId(MetadataId),
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
    #[error("Attachment upload timed out")]
    Timeout,
    #[error("Storage quota exceeded")]
    StorageQuotaExceeded,
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
    #[error("Attachment missing key packets")]
    AttachmentMissingKeyPackets(LocalAttachmentId),
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

impl From<RecipientError> for MailContextError {
    fn from(err: RecipientError) -> Self {
        MailContextError::Draft(Error::Recipient(err))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentDispositionSwapError {
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Attachment {0} not found")]
    AttachmentNotFound(LocalAttachmentId),
    #[error("Attachment with cid:{0} not found")]
    AttachmentNotFoundCid(ContentId),
    #[error("Attachment {0} has no remote id")]
    AttachmentHasNoRemoteId(LocalAttachmentId),
    #[error("Draft attachment {0} metadata not found")]
    AttachmentMetadataNotFound(LocalAttachmentId),
    #[error("Attempting to swap disposition to the same state")]
    Noop,
    #[error("Draft {0} does not have local message id")]
    NoMessageIdInDraftMetadata(MetadataId),
    #[error("Invalid state for attachment {0}")]
    InvalidState(LocalAttachmentId),
    #[error("Attachment {0} has no content id")]
    AttachmentHasNoContentId(LocalAttachmentId),
    #[error("Attachment {0} does not exist on server")]
    AttachmentDoesNotExistServer(AttachmentId),
    #[error("Attachment {0} message does not exist on server")]
    AttachmentMessageDoesNotExist(AttachmentId),
    #[error("Attachment {0} message is not a draft")]
    AttachmentMessageIsNotADraft(AttachmentId),
    #[error("Attachment {0} does not have a valid cid")]
    AttachmentDoesNotHaveValidCid(AttachmentId),
}

impl From<AttachmentDispositionSwapError> for MailContextError {
    fn from(value: AttachmentDispositionSwapError) -> Self {
        Self::Draft(Error::AttachmentDispositionSwap(value))
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

/// Indicates the status of syncing a draft.
///
/// By default we always sync the draft bodies from the server, but if there is no network
/// we will serve the local cached version.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DraftSyncStatus {
    /// We managed to sync the draft body from the server
    Synced,
    /// We only have a cached version available.
    Cached,
}

#[derive(Debug, Clone)]
pub struct DraftState {
    pub sender: String,
    pub to_list: RecipientList,
    pub cc_list: RecipientList,
    pub bcc_list: RecipientList,
    pub address_id: AddressId,
    pub subject: String,
    pub send_result: Option<DraftSendResult>,
    pub body: String,
    pub mime_type: MessageMimeType,
}

impl DraftState {
    fn from_draft(draft: &draft_v1::Draft) -> Self {
        Self {
            sender: draft.sender.clone(),
            to_list: draft.to_list.clone(),
            cc_list: draft.cc_list.clone(),
            bcc_list: draft.bcc_list.clone(),
            address_id: draft.address_id.clone(),
            subject: draft.subject.clone(),
            send_result: draft.send_result.clone(),
            body: draft.body().to_owned(),
            mime_type: draft.mime_type(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DraftEvent {
    RecipientListUpdated {
        group: RecipientGroupId,
        list: RecipientList,
    },
    RecipientListsUpdated {
        to: RecipientList,
        cc: RecipientList,
        bcc: RecipientList,
    },
    Sent,
    Discarded,
}

#[derive(Clone)]
pub struct DraftActor {
    sender: mpsc::Sender<DraftActorMessage>,
    pub metadata_id: MetadataId,
    event_sender: broadcast::Sender<DraftEvent>,
}

impl fmt::Debug for DraftActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DraftActor{{{:?}}}", self.metadata_id)
    }
}

const DRAFT_EVENT_CHANNEL_CAPACITY: usize = 8;

impl DraftActor {
    pub fn subscribe(&self) -> broadcast::Receiver<DraftEvent> {
        self.event_sender.subscribe()
    }

    pub async fn sender_addresses(&self) -> Result<Vec<Address>, MailContextError> {
        self.act(DraftActorMessage::SenderAddresses).await?
    }

    pub async fn schedule_send_options(
        ctx: &MailUserContext,
    ) -> MailContextResult<ScheduleSendOptions<Local>> {
        draft_v1::Draft::schedule_send_options(ctx).await
    }

    pub async fn open(
        context: &MailUserContext,
        message_id: LocalMessageId,
    ) -> Result<(Self, DraftSyncStatus), MailContextError> {
        Self::open_ex(context, message_id, DraftActorOptions::default()).await
    }

    pub async fn open_ex(
        context: &MailUserContext,
        message_id: LocalMessageId,
        options: DraftActorOptions,
    ) -> Result<(Self, DraftSyncStatus), MailContextError> {
        let (draft, sync_status) = draft_v1::Draft::open(context, message_id).await?;
        let draft = Self::create(context, draft, options);
        if sync_status == DraftSyncStatus::Synced {
            draft
                .sender
                .send(DraftActorMessage::RevalidateAllRecipients)
                .await
                .map_err(|_| Error::Actor)?;
        }
        Ok((draft, sync_status))
    }

    pub async fn empty(context: &MailUserContext) -> Result<Self, MailContextError> {
        Self::empty_ex(context, DraftActorOptions::default()).await
    }

    pub async fn empty_ex(
        context: &MailUserContext,
        options: DraftActorOptions,
    ) -> Result<Self, MailContextError> {
        let draft = draft_v1::Draft::empty(context).await?;

        Ok(Self::create(context, draft, options))
    }

    #[instrument(skip_all)]
    pub async fn mailto(
        context: &MailUserContext,
        options: DraftActorOptions,
        mailto: Mailto,
    ) -> Result<Self, MailContextError> {
        info!("Creating draft from a mailto link");

        let draft = Self::empty_ex(context, options).await?;

        let recipients = {
            let tos = mailto
                .to
                .into_iter()
                .map(|email| (RecipientGroupId::To, email));

            let ccs = mailto
                .cc
                .into_iter()
                .map(|email| (RecipientGroupId::Cc, email));

            let bccs = mailto
                .bcc
                .into_iter()
                .map(|email| (RecipientGroupId::Bcc, email));

            tos.chain(ccs).chain(bccs)
        };

        for (group, email) in recipients {
            let result = draft
                .add_single_recipient(
                    group,
                    RecipientEntry {
                        name: None,
                        email: email.into(),
                    },
                )
                .await;

            if let Err(err) = result {
                warn!(?err, "Couldn't add recipient, continuing without it");
            }
        }

        if let Some(subject) = mailto.subject {
            let result = draft.set_subject(subject).await;

            if let Err(err) = result {
                warn!(?err, "Couldn't set subject, continuing without it");
            }
        }

        if let Some(body) = mailto.body {
            let result = draft.set_body(body).await;

            if let Err(err) = result {
                warn!(?err, "Couldn't set body, continuing without it");
            }
        }

        Ok(draft)
    }

    #[instrument(skip_all)]
    pub async fn from_ios_share_extension(
        context: &MailUserContext,
        options: DraftActorOptions,
    ) -> Result<Self, MailContextError> {
        info!("Creating a draft from share extension's stub-draft");

        let mail_cache_path = context.mail_context().mail_cache_path();

        let draft = IosShareExtension::load_draft(mail_cache_path)?.ok_or_else(|| {
            warn!("Whoopsie, looks like there's no stub-draft to load");
            OpenError::ShareExtensionStubDraftMissing
        })?;

        let mut body = String::new();
        let mut tether = context.user_stash().connection().await?;

        let address_id = context
            .user_context()
            .address_service()
            .find_valid_sender_address()
            .await?
            .ok_or(OpenError::UserHasNoAddresses)?
            .remote_id
            .unwrap();

        let mime_type = MailSettings::get_or_default(&tether).await.draft_mime_type;

        // ---
        // Create attachments

        let atts = {
            let atts = draft
                .attachments
                .into_iter()
                .map(|att| (att, Disposition::Attachment));

            // If the draft's mime type doesn't support inline attachments,
            // force them to be normal ones
            let inline_atts_disp = if mime_type.supports_inline_attachments() {
                Disposition::Inline
            } else {
                Disposition::Attachment
            };

            let inline_atts = draft
                .inline_attachments
                .into_iter()
                .map(move |att| (att, inline_atts_disp));

            atts.chain(inline_atts)
        };

        let mut created_atts = Vec::new();

        for (att_idx, (att, att_disp)) in atts.enumerate() {
            debug!(?att_idx, "Creating attachment");

            let att = Attachment::create_local(
                context,
                address_id.clone(),
                att_disp,
                &att.path,
                att.name,
                &mut tether,
            )
            .await?;

            if let Some(img) = att.as_inline_img() {
                body.push_str(&img);
            }

            created_atts.push(att);
        }

        // ---
        // Create draft itself

        let this = Self::empty_ex(context, options).await?;

        for att in created_atts {
            this.add_attachment(&att).await?;
        }

        if let Some(subject) = draft.subject {
            this.set_subject(subject).await?;
        }

        this.set_body({
            if let Some(user_body) = draft.body {
                body.push_str(&user_body);
            }

            let signature = this.body().await?;

            if !signature.is_empty() {
                body.push_str(&signature);
            }

            body
        })
        .await?;

        // ---

        IosShareExtension::delete_draft(mail_cache_path);

        Ok(this)
    }

    pub async fn reply(
        context: &MailUserContext,
        message_id: LocalMessageId,
        reply_mode: ReplyMode,
        use_utc: bool,
    ) -> Result<Self, MailContextError> {
        Self::reply_ex(
            context,
            message_id,
            reply_mode,
            use_utc,
            DraftActorOptions::default(),
        )
        .await
    }

    pub async fn reply_ex(
        context: &MailUserContext,
        message_id: LocalMessageId,
        reply_mode: ReplyMode,
        use_utc: bool,
        options: DraftActorOptions,
    ) -> Result<Self, MailContextError> {
        let draft = draft_v1::Draft::reply(context, message_id, reply_mode, use_utc).await?;

        Ok(Self::create(context, draft, options))
    }

    pub async fn save(&self) -> Result<QueuedActionOutput<Save>, MailContextError> {
        self.act(|sender| DraftActorMessage::Save { sender })
            .await?
    }

    pub async fn send(&self) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.act(|sender| DraftActorMessage::Send { sender })
            .await?
    }

    pub async fn schedule_send(
        &self,
        delivery_time: DateTime<Local>,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.act(|sender| DraftActorMessage::ScheduleSend {
            delivery_time,
            sender,
        })
        .await?
    }

    pub async fn discard(&self) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        self.act(|sender| DraftActorMessage::Discard { sender })
            .await?
    }

    pub async fn message_id(&self) -> Result<Option<LocalMessageId>, MailContextError> {
        self.act(DraftActorMessage::GetMessageId).await?
    }

    pub async fn conversation_id(&self) -> Result<Option<LocalConversationId>, MailContextError> {
        self.act(DraftActorMessage::GetConversationId).await?
    }

    pub async fn load_image(
        &self,
        url: String,
        policy: ImagePolicy,
    ) -> MailContextResult<AttachmentData> {
        self.act(|sender| DraftActorMessage::LoadImage {
            url,
            policy,
            sender,
        })
        .await?
    }

    pub async fn delete_attachment_if_in_staging_area(&self, ctx: &MailUserContext, path: &Path) {
        let staging_path = self.attachment_staging_path(ctx);

        if path.starts_with(&staging_path)
            && let Err(e) = fs::remove_file(&staging_path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            // This is a warning as the background process will try again.
            warn!("Failed to delete attachment from staging area at {path:?}: {e:?}");
        }
    }
    pub async fn add_attachment(
        &self,
        attachment: &Attachment,
    ) -> Result<ActionId, MailContextError> {
        self.act(|sender| DraftActorMessage::AddAttachment {
            attachment_id: attachment.local_id.unwrap(),
            sender,
        })
        .await?
    }

    pub async fn remove_attachment(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveAttachment {
            attachment_id,
            sender,
        })
        .await?
    }

    pub async fn remove_attachment_with_cid(
        &self,
        content_id: ContentId,
    ) -> Result<ActionId, MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveAttachmentWithCid {
            content_id: content_id.clone(),
            sender,
        })
        .await?
    }

    pub async fn retry_attachment_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        self.act(|sender| DraftActorMessage::RetryAttachmentOperation {
            attachment_id,
            sender,
        })
        .await?
    }
    pub fn attachment_staging_path(&self, context: &MailUserContext) -> PathBuf {
        draft_v1::draft_attachment_staging_path(context, self.metadata_id)
    }

    pub async fn attachments(&self) -> Result<Vec<DraftAttachment>, MailContextError> {
        self.act(DraftActorMessage::GetAttachments).await?
    }

    pub async fn composer_content(
        &self,
        theme_opts: ThemeOpts,
        editor_id: String,
    ) -> Result<(String, String), MailContextError> {
        self.act(|sender| DraftActorMessage::ComposerContent {
            theme_opts,
            editor_id,
            sender,
        })
        .await
    }

    pub async fn body(&self) -> Result<String, MailContextError> {
        self.act(DraftActorMessage::GetBody).await
    }

    pub async fn set_body(&self, body: String) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetBody { body, sender })
            .await?
    }

    pub async fn mime_type(&self) -> Result<MessageMimeType, MailContextError> {
        self.act(DraftActorMessage::GetMimeType).await
    }

    pub async fn set_mime_type(&self, mime_type: MessageMimeType) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetMimeType { sender, mime_type })
            .await
    }

    pub async fn sanitize_body(&self) -> Result<(), MailContextError> {
        self.act(DraftActorMessage::SanitizeBody).await?
    }

    pub async fn cancel_schedule_send(
        ctx: &MailUserContext,
        message_id: LocalMessageId,
    ) -> MailContextResult<DateTime<Local>> {
        draft_v1::Draft::cancel_schedule_send(ctx, message_id).await
    }

    // Returns updated body with the new signature if any - uniffi compatability method, to be
    // removed in the future.
    pub async fn change_sender_address(&self, email: String) -> Result<String, MailContextError> {
        self.act(move |sender| DraftActorMessage::ChangeSenderAddress { email, sender })
            .await?
    }
    pub async fn is_password_protected(&self) -> Result<bool, MailContextError> {
        self.act(DraftActorMessage::IsPasswordProtected).await?
    }
    pub async fn get_password(&self) -> Result<Option<EoData>, MailContextError> {
        self.act(DraftActorMessage::GetPassword).await?
    }

    pub async fn set_password(
        &self,
        password: &str,
        hint: Option<String>,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetPassword {
            sender,
            password: SecretString::new(String::from(password)),
            hint,
        })
        .await?
    }

    pub async fn set_password_with_secret(
        &self,
        password: SecretString,
        hint: Option<String>,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetPassword {
            sender,
            password,
            hint,
        })
        .await?
    }
    pub async fn remove_password(&self) -> Result<(), MailContextError> {
        self.act(DraftActorMessage::RemovePassword).await?
    }
    pub async fn set_expiration_time(
        &self,
        expiration_time: DraftExpirationTime,
    ) -> Result<(), MailContextError> {
        self.act(move |sender| DraftActorMessage::SetExpirationTime {
            time: expiration_time,
            sender,
        })
        .await?
    }
    pub async fn expiration_time(&self) -> Result<DraftExpirationTime, MailContextError> {
        self.act(DraftActorMessage::GetExpirationTime).await?
    }

    #[cfg(feature = "test-utils")]
    pub async fn test_mutate(
        &self,
        closure: impl FnOnce(&mut draft_v1::Draft) + Send + 'static,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::TestMutate {
            mutate: Box::new(closure),
            sender,
        })
        .await
    }

    pub async fn address_id(&self) -> Result<AddressId, MailContextError> {
        self.act(DraftActorMessage::GetAddressId).await
    }

    pub async fn sender(&self) -> Result<String, MailContextError> {
        self.act(DraftActorMessage::GetSender).await
    }

    pub async fn address_validation_result(
        &self,
    ) -> Result<Option<DraftAddressValidationResult>, MailContextError> {
        self.act(DraftActorMessage::GetAddressValidationResult)
            .await
    }

    pub async fn clear_address_validation_result(&self) -> Result<(), MailContextError> {
        self.act(DraftActorMessage::ClearAddressValidationResult)
            .await
    }

    pub async fn take_address_validation_result(
        &self,
    ) -> Result<Option<DraftAddressValidationResult>, MailContextError> {
        self.act(DraftActorMessage::TakeAddressValidationResult)
            .await
    }

    pub async fn add_single_recipient(
        &self,
        group: RecipientGroupId,
        recipient: RecipientEntry,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::AddSingleRecipient {
            group,
            recipient,
            sender,
        })
        .await?
    }

    pub async fn add_recipient_to_group(
        &self,
        group: RecipientGroupId,
        group_name: NonEmptyString,
        recipients: impl IntoIterator<Item = RecipientEntry>,
        total_in_group: u64,
    ) -> Result<Vec<RecipientEntry>, MailContextError> {
        self.act(|sender| DraftActorMessage::AddRecipientGroup {
            group,
            group_name,
            recipients: recipients.into_iter().collect(),
            total_in_group,
            sender,
        })
        .await?
    }

    pub async fn set_recipients(
        &self,
        to: RecipientList,
        cc: RecipientList,
        bcc: RecipientList,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetRecipientLists {
            to,
            cc,
            bcc,
            sender,
        })
        .await?
    }

    pub async fn remove_single_recipient(
        &self,
        group: RecipientGroupId,
        email: PrivateEmail,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveSingleRecipient {
            group,
            email,
            sender,
        })
        .await?
    }

    pub async fn remove_recipient_from_group(
        &self,
        group: RecipientGroupId,
        email: PrivateEmail,
        group_name: NonEmptyString,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveGroupRecipient {
            group,
            email,
            group_name,
            sender,
        })
        .await?
    }

    pub async fn remove_recipients_from_group(
        &self,
        group: RecipientGroupId,
        emails: impl IntoIterator<Item = PrivateEmail>,
        group_name: NonEmptyString,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveGroupRecipients {
            group,
            emails: emails.into_iter().collect(),
            group_name,
            sender,
        })
        .await?
    }

    pub async fn remove_recipient_group(
        &self,
        group: RecipientGroupId,
        group_name: NonEmptyString,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::RemoveRecipientGroup {
            group,
            group_name,
            sender,
        })
        .await?
    }

    pub async fn recipients(
        &self,
        group: RecipientGroupId,
    ) -> Result<Vec<Recipient>, MailContextError> {
        self.act(|sender| DraftActorMessage::GetRecipients { group, sender })
            .await
    }

    pub async fn state(&self) -> Result<DraftState, MailContextError> {
        self.act(DraftActorMessage::GetState).await
    }

    pub async fn set_subject(&self, subject: String) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetSubject { sender, subject })
            .await?
    }

    pub async fn subject(&self) -> Result<String, MailContextError> {
        self.act(DraftActorMessage::GetSubject).await
    }

    pub async fn to_list(&self) -> Result<RecipientList, MailContextError> {
        self.act(|sender| DraftActorMessage::GetRecipientList {
            group: RecipientGroupId::To,
            sender,
        })
        .await
    }

    pub async fn cc_list(&self) -> Result<RecipientList, MailContextError> {
        self.act(|sender| DraftActorMessage::GetRecipientList {
            group: RecipientGroupId::Cc,
            sender,
        })
        .await
    }
    pub async fn bcc_list(&self) -> Result<RecipientList, MailContextError> {
        self.act(|sender| DraftActorMessage::GetRecipientList {
            group: RecipientGroupId::Bcc,
            sender,
        })
        .await
    }

    pub async fn set_to_list(&self, recipients: RecipientList) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetRecipientList {
            group: RecipientGroupId::To,
            recipients,
            sender,
        })
        .await?
    }

    pub async fn set_cc_list(&self, recipients: RecipientList) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetRecipientList {
            group: RecipientGroupId::Cc,
            recipients,
            sender,
        })
        .await?
    }

    pub async fn set_bcc_list(&self, recipients: RecipientList) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SetRecipientList {
            group: RecipientGroupId::Bcc,
            recipients,
            sender,
        })
        .await?
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn remote_create(
        context: &MailUserContext,
        session: &Session,
        address_id: AddressId,
        save_action: &Save,
        attachments: &[Attachment],
        message_body: &str,
        draft_reply_or_forward_params: Option<DraftReplyOrForwardParams>,
        tether: &Tether,
    ) -> Result<ApiMessage, MailContextError> {
        draft_v1::Draft::remote_create(
            context,
            session,
            address_id,
            save_action,
            attachments,
            message_body,
            draft_reply_or_forward_params,
            tether,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn remote_update(
        context: &MailUserContext,
        session: &Session,
        address_id: AddressId,
        local_message_id: LocalMessageId,
        message_id: MessageId,
        save_action: &Save,
        attachments: &[Attachment],
        message_body: &str,
        tether: &Tether,
    ) -> Result<ApiMessage, MailContextError> {
        draft_v1::Draft::remote_update(
            context,
            session,
            address_id,
            local_message_id,
            message_id,
            save_action,
            attachments,
            message_body,
            tether,
        )
        .await
    }
    pub async fn action_discard(
        message_id: LocalMessageId,
        tether: &Tether,
        queue: &Queue,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        draft_v1::Draft::action_discard(message_id, tether, queue, origin).await
    }

    pub async fn action_undo_send(
        queue: &Queue,
        message_id: LocalMessageId,
    ) -> Result<QueuedActionOutput<UndoSend>, ActionError<UndoSend>> {
        draft_v1::Draft::action_undo_send(queue, message_id).await
    }

    pub async fn validate_expiration_feature(
        &self,
    ) -> Result<ExpirationFeatureSupportReport, MailContextError> {
        self.act(DraftActorMessage::ValidateExpirationFeature)
            .await?
    }

    pub async fn swap_attachment_disposition(
        &self,
        attachment_id: LocalAttachmentId,
        new_attachment_disposition: Disposition,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SwapAttachmentDisposition {
            attachment_id,
            new_disposition: match new_attachment_disposition {
                Disposition::Attachment => CombinedAttachmentDisposition::Attachment,
                Disposition::Inline => CombinedAttachmentDisposition::Inline(ContentId::new()),
            },
            sender,
        })
        .await?
    }
    pub async fn swap_attachment_disposition_from_inline(
        &self,
        content_id: ContentId,
    ) -> Result<(), MailContextError> {
        self.act(|sender| DraftActorMessage::SwapAttachmentDispositionCid { content_id, sender })
            .await?
    }
}

#[derive(Display)]
enum DraftActorMessage {
    #[display("GetExpirationTime")]
    GetExpirationTime(oneshot::Sender<Result<DraftExpirationTime, MailContextError>>),

    #[display("SetExpirationTime")]
    SetExpirationTime {
        time: DraftExpirationTime,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("RemovePassword")]
    RemovePassword(oneshot::Sender<Result<(), MailContextError>>),

    #[display("SetPassword")]
    SetPassword {
        password: SecretString,
        hint: Option<String>,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("GetPassword")]
    GetPassword(oneshot::Sender<Result<Option<EoData>, MailContextError>>),

    #[display("IsPasswordProtected")]
    IsPasswordProtected(oneshot::Sender<Result<bool, MailContextError>>),

    #[display("ChangeSenderAddress")]
    ChangeSenderAddress {
        email: String,
        sender: oneshot::Sender<Result<String, MailContextError>>,
    },

    #[display("SanitizeBody")]
    SanitizeBody(oneshot::Sender<Result<(), MailContextError>>),

    #[display("SetMimeType")]
    SetMimeType {
        mime_type: MessageMimeType,
        sender: oneshot::Sender<()>,
    },

    #[display("GetMimeType")]
    GetMimeType(oneshot::Sender<MessageMimeType>),

    #[display("SetBody")]
    SetBody {
        body: String,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("GetBody")]
    GetBody(oneshot::Sender<String>),

    #[display("ComposerContent")]
    ComposerContent {
        theme_opts: ThemeOpts,
        editor_id: String,
        sender: oneshot::Sender<(String, String)>,
    },

    #[display("GetAttachments")]
    GetAttachments(oneshot::Sender<Result<Vec<DraftAttachment>, MailContextError>>),

    #[display("RetryAttachmentUpload")]
    RetryAttachmentOperation {
        attachment_id: LocalAttachmentId,
        sender: oneshot::Sender<Result<ActionId, MailContextError>>,
    },

    #[display("RemoveAttachmentWithCid")]
    RemoveAttachmentWithCid {
        content_id: ContentId,
        sender: oneshot::Sender<Result<ActionId, MailContextError>>,
    },

    #[display("RemoveAttachment")]
    RemoveAttachment {
        attachment_id: LocalAttachmentId,
        sender: oneshot::Sender<Result<ActionId, MailContextError>>,
    },

    #[display("AddAttachment")]
    AddAttachment {
        attachment_id: LocalAttachmentId,
        sender: oneshot::Sender<Result<ActionId, MailContextError>>,
    },

    #[display("LoadImage")]
    LoadImage {
        url: String,
        policy: ImagePolicy,
        sender: oneshot::Sender<Result<AttachmentData, MailContextError>>,
    },

    #[display("GetMessageId")]
    GetMessageId(oneshot::Sender<Result<Option<LocalMessageId>, MailContextError>>),

    #[display("GetConversationId")]
    GetConversationId(oneshot::Sender<Result<Option<LocalConversationId>, MailContextError>>),

    #[display("Discard")]
    Discard {
        sender: oneshot::Sender<Result<QueuedActionOutput<Discard>, MailContextError>>,
    },

    #[display("ScheduleSend")]
    ScheduleSend {
        delivery_time: DateTime<Local>,
        sender: oneshot::Sender<Result<QueuedActionOutput<draft::Send>, MailContextError>>,
    },

    #[display("Send")]
    Send {
        sender: oneshot::Sender<Result<QueuedActionOutput<draft::Send>, MailContextError>>,
    },

    #[display("Save")]
    Save {
        sender: oneshot::Sender<Result<QueuedActionOutput<draft::Save>, MailContextError>>,
    },

    #[display("SenderAddresses")]
    SenderAddresses(oneshot::Sender<Result<Vec<Address>, MailContextError>>),

    #[display("GetAddressId")]
    GetAddressId(oneshot::Sender<AddressId>),

    #[display("GetSender")]
    GetSender(oneshot::Sender<String>),

    #[display("GetAddressValidationResult")]
    GetAddressValidationResult(oneshot::Sender<Option<DraftAddressValidationResult>>),

    #[display("ClearAddressValidationResult")]
    ClearAddressValidationResult(oneshot::Sender<()>),

    #[display("TakeAddressValidationResult")]
    TakeAddressValidationResult(oneshot::Sender<Option<DraftAddressValidationResult>>),

    #[display("AddSingleRecipient")]
    AddSingleRecipient {
        group: RecipientGroupId,
        recipient: RecipientEntry,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("AddRecipientGroup")]
    AddRecipientGroup {
        group: RecipientGroupId,
        group_name: NonEmptyString,
        recipients: Vec<RecipientEntry>,
        total_in_group: u64,
        sender: oneshot::Sender<Result<Vec<RecipientEntry>, MailContextError>>,
    },

    #[display("SetRecipientLists")]
    SetRecipientLists {
        to: RecipientList,
        cc: RecipientList,
        bcc: RecipientList,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("RemoveSingleRecipient")]
    RemoveSingleRecipient {
        group: RecipientGroupId,
        email: PrivateEmail,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("RemoveRecipientFromGroup")]
    RemoveGroupRecipient {
        group: RecipientGroupId,
        email: PrivateEmail,
        group_name: NonEmptyString,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("RemoveRecipientsFromGroup")]
    RemoveGroupRecipients {
        group: RecipientGroupId,
        emails: Vec<PrivateEmail>,
        group_name: NonEmptyString,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("RemoveRecipientGroup")]
    RemoveRecipientGroup {
        group: RecipientGroupId,
        group_name: NonEmptyString,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("GetRecipients")]
    GetRecipients {
        group: RecipientGroupId,
        sender: oneshot::Sender<Vec<Recipient>>,
    },

    #[cfg(feature = "test-utils")]
    #[display("TestMutate")]
    TestMutate {
        mutate: Box<dyn FnOnce(&mut draft_v1::Draft) + Send + 'static>,
        sender: oneshot::Sender<()>,
    },

    #[display("GetState")]
    GetState(oneshot::Sender<DraftState>),

    #[display("SetSubject")]
    SetSubject {
        subject: String,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("GetSubject")]
    GetSubject(oneshot::Sender<String>),

    #[display("GetRecipientList")]
    GetRecipientList {
        group: RecipientGroupId,
        sender: oneshot::Sender<RecipientList>,
    },

    #[display("SetRecipientList")]
    SetRecipientList {
        group: RecipientGroupId,
        recipients: RecipientList,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("OnRecipientValidation")]
    OnRecipientValidation {
        group: RecipientGroupId,
        updates: RecipientValidationUpdate,
    },

    #[display("ReValidateAllRecipients")]
    RevalidateAllRecipients,

    #[display("ValidateExpirationFeature")]
    ValidateExpirationFeature(
        oneshot::Sender<Result<ExpirationFeatureSupportReport, MailContextError>>,
    ),

    #[display("SwapAttachmentDisposition")]
    SwapAttachmentDisposition {
        attachment_id: LocalAttachmentId,
        new_disposition: CombinedAttachmentDisposition,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },

    #[display("SwapAttachmentDispositionCid")]
    SwapAttachmentDispositionCid {
        content_id: ContentId,
        sender: oneshot::Sender<Result<(), MailContextError>>,
    },
}

#[derive(Debug, Copy, Clone)]
pub enum RecipientGroupId {
    To,
    Cc,
    Bcc,
}

#[derive(Default, Debug, Clone)]
pub struct DraftActorOptions {
    pub address_validation_enabled: bool,
    pub auto_save_every: Option<Duration>,
}

impl DraftActorOptions {
    fn auto_save_enabled(&self) -> bool {
        self.auto_save_every.is_some()
    }
}

impl DraftActor {
    fn create(ctx: &MailUserContext, draft: draft_v1::Draft, options: DraftActorOptions) -> Self {
        let metadata_id = draft.metadata_id;
        let (sender, receiver) = mpsc::channel(2);
        let (event_sender, _) = broadcast::channel(DRAFT_EVENT_CHANNEL_CAPACITY);

        let weak = ctx.as_weak();
        let cancellation_token = ctx.core_context().cancellation_token().child_token();
        let cloned_event_sender = event_sender.clone();
        let cloned_sender = sender.clone();

        ctx.spawn(async move {
            Self::background_loop(
                weak,
                receiver,
                cloned_sender,
                draft,
                cloned_event_sender,
                options,
                cancellation_token,
            )
            .await
        });

        Self {
            sender,
            metadata_id,
            event_sender,
        }
    }

    async fn act<T: Send>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<T>) -> DraftActorMessage,
    ) -> Result<T, MailContextError> {
        let (sender, receiver) = oneshot::channel::<T>();
        let msg = build_message(sender);
        tracing::trace!("Sending message: {msg}");
        self.sender.send(msg).await.map_err(|_| Error::Actor)?;
        tracing::trace!("Awaiting reply");
        let r = receiver.await.map_err(|_| Error::Actor)?;
        tracing::trace!("Reply received");
        Ok(r)
    }

    #[tracing::instrument(name="draft_actor",skip_all, fields(id=%draft.metadata_id))]
    async fn background_loop(
        ctx: Weak<MailUserContext>,
        mut actor_receiver: mpsc::Receiver<DraftActorMessage>,
        actor_sender: mpsc::Sender<DraftActorMessage>,
        mut draft: draft_v1::Draft,
        event_sender: broadcast::Sender<DraftEvent>,
        options: DraftActorOptions,
        cancellation_token: CancellationToken,
    ) {
        let publish_event = |event: DraftEvent| {
            // sending on broadcast only fails if there are no receivers
            let _ = event_sender.send(event);
        };

        let mut auto_saver = DraftAutoSaver::default();

        tracing::info!("Starting");
        while let Some(message) = actor_receiver.recv().await {
            let Some(ctx) = ctx.upgrade() else {
                tracing::error!("Mail User Context is dead, terminating");
                return;
            };

            tracing::debug!("Received message: {message}");

            match message {
                DraftActorMessage::GetExpirationTime(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.expiration_time(&tether).await
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SetExpirationTime { time, sender } => {
                    let r = async {
                        let mut tether = ctx.user_stash().connection().await?;
                        draft.set_expiration_time(&mut tether, time).await
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::RemovePassword(sender) => {
                    let r = async {
                        let mut tether = ctx.user_stash().connection().await?;
                        draft.remove_password(&mut tether).await
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SetPassword {
                    password,
                    hint,
                    sender,
                } => {
                    let r = draft
                        .set_password(&ctx, password.expose_secret(), hint)
                        .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::GetPassword(sender) => {
                    let r = draft.get_password(&ctx).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::IsPasswordProtected(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.is_password_protected(&tether).await
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::ChangeSenderAddress { email, sender } => {
                    let r = draft
                        .change_sender_address(&ctx, email)
                        .await
                        .map(|_| draft.body().to_owned());
                    let r = auto_saver.map_save(r, &ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SanitizeBody(sender) => {
                    draft.sanitize_body();
                    let r = auto_saver.periodic_save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SetMimeType { mime_type, sender } => {
                    draft.set_mime_type(mime_type);
                    let _ = sender.send(());
                }

                DraftActorMessage::GetMimeType(sender) => {
                    let _ = sender.send(draft.mime_type());
                }

                DraftActorMessage::SetBody { body, sender } => {
                    let r = if body != draft.body() {
                        draft.set_body(body);
                        auto_saver.periodic_save(&ctx, &mut draft, &options).await
                    } else {
                        Ok(())
                    };
                    let _ = sender.send(r);
                }

                DraftActorMessage::GetBody(sender) => {
                    let _ = sender.send(draft.body().to_owned());
                }

                DraftActorMessage::ComposerContent {
                    theme_opts,
                    editor_id,
                    sender,
                } => {
                    let _ = sender.send((
                        draft.html_head_content_for_composer(theme_opts, editor_id),
                        draft.body().to_owned(),
                    ));
                }

                DraftActorMessage::GetAttachments(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.attachments(&tether).await
                    }
                    .await;
                    let _ = sender.send(r.map_err(Into::into));
                }

                DraftActorMessage::RetryAttachmentOperation {
                    attachment_id,
                    sender,
                } => {
                    let r = draft.retry_attachment_operation(&ctx, attachment_id).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::RemoveAttachmentWithCid { content_id, sender } => {
                    let r = draft.remove_attachment_with_cid(&ctx, content_id).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::RemoveAttachment {
                    attachment_id,
                    sender,
                } => {
                    let r = draft.remove_attachment(&ctx, attachment_id).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::AddAttachment {
                    attachment_id,
                    sender,
                } => {
                    let r = draft.add_attachment(&ctx, attachment_id).await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::LoadImage {
                    url,
                    policy,
                    sender,
                } => {
                    // We don't wait to wait for this to finish so we can run in parallel
                    let id = draft.metadata_id;

                    ctx.spawn_ex(async move |ctx| {
                        let r = draft_v1::Draft::load_image(id, &ctx, &url, policy).await;
                        let _ = sender.send(r);
                    });
                }

                DraftActorMessage::GetMessageId(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.message_id(&tether).await.map_err(Into::into)
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::GetConversationId(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.conversation_id(&tether).await.map_err(Into::into)
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::Discard { sender } => {
                    let r = draft.discard(ctx.action_queue(), ctx.origin()).await;
                    if r.is_ok() {
                        // cancel pending validation task
                        cancellation_token.cancel();
                        publish_event(DraftEvent::Discarded);
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::ScheduleSend {
                    delivery_time,
                    sender,
                } => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        let queue = ctx.action_queue();
                        draft
                            .schedule_send(delivery_time, queue, &tether, ctx.origin())
                            .await
                    }
                    .await;
                    if r.is_ok() {
                        // sending currently always saves
                        auto_saver.reset_save_state();
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::Send { sender } => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        let queue = ctx.action_queue();
                        draft.send(queue, &tether, ctx.origin()).await
                    }
                    .await;
                    if r.is_ok() {
                        // sending currently always saves
                        auto_saver.reset_save_state();
                        // cancel pending validation task
                        cancellation_token.cancel();
                        publish_event(DraftEvent::Sent);
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::SenderAddresses(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        draft.sender_addresses(&tether).await
                    }
                    .await;
                    let _ = sender.send(r.map_err(Into::into));
                }

                DraftActorMessage::Save { sender } => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;
                        let queue = ctx.action_queue();
                        draft.save(queue, &tether, ctx.origin()).await
                    }
                    .await;
                    if r.is_ok() {
                        auto_saver.reset_save_state();
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::GetAddressId(sender) => {
                    let _ = sender.send(draft.address_id.clone());
                }

                DraftActorMessage::GetSender(sender) => {
                    let _ = sender.send(draft.sender.clone());
                }

                DraftActorMessage::GetAddressValidationResult(sender) => {
                    let _ = sender.send(draft.address_validation_result.clone());
                }

                DraftActorMessage::ClearAddressValidationResult(sender) => {
                    draft.address_validation_result = None;
                    let _ = sender.send(());
                }

                DraftActorMessage::TakeAddressValidationResult(sender) => {
                    let _ = sender.send(draft.address_validation_result.take());
                }

                DraftActorMessage::AddSingleRecipient {
                    group,
                    recipient,
                    sender,
                } => {
                    let email = recipient.email.clone();
                    let r = {
                        let list = recipient_group_from_draft(&mut draft, group);
                        if options.address_validation_enabled {
                            DraftOnRecipientValidation::new_list(
                                group,
                                actor_sender.clone(),
                                list,
                                cancellation_token.clone(),
                            )
                            .add_single(&ctx, recipient)
                        } else {
                            match list.add_single(recipient) {
                                Ok(_) => Ok(()),
                                Err(e) => Err(e),
                            }
                        }
                    };
                    if r.is_err() {
                        // do not save if there is an error
                        let _ = sender.send(r.map_err(Into::into));
                        continue;
                    }
                    let r = auto_saver.map_save(r, &ctx, &mut draft, &options).await;
                    let list = recipient_group_from_draft(&mut draft, group);
                    if r.is_ok() {
                        publish_event(DraftEvent::RecipientListUpdated {
                            group,
                            list: list.clone(),
                        });
                    } else {
                        list.remove_single(email.as_clear_text_str());
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::AddRecipientGroup {
                    group,
                    group_name,
                    recipients,
                    total_in_group,
                    sender,
                } => {
                    let recipient_emails = recipients
                        .iter()
                        .map(|e| e.email.clone().into_clear_text_string())
                        .collect::<Vec<_>>();
                    let duplicates = {
                        let list = recipient_group_from_draft(&mut draft, group);
                        if options.address_validation_enabled {
                            DraftOnRecipientValidation::new_list(
                                group,
                                actor_sender.clone(),
                                list,
                                cancellation_token.clone(),
                            )
                            .add_group(
                                &ctx,
                                group_name.clone(),
                                recipients,
                                total_in_group,
                            )
                        } else {
                            let (_, duplicates) =
                                list.add_group(group_name.clone(), recipients, total_in_group);
                            duplicates
                        }
                    };
                    let r = auto_saver
                        .save(&ctx, &mut draft, &options)
                        .await
                        .map(|_| duplicates);
                    let list = recipient_group_from_draft(&mut draft, group);
                    if r.is_err() {
                        // match behavior in uniffi
                        list.remove_group_recipients(&group_name, recipient_emails);
                    }
                    let _ = sender.send(r);
                    // Still issue an update as group total could have changed.
                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::SetRecipientLists {
                    to,
                    cc,
                    bcc,
                    sender,
                } => {
                    //TODO: handle swap
                    draft.to_list = to;
                    draft.cc_list = cc;
                    draft.bcc_list = bcc;

                    if options.address_validation_enabled {
                        for id in [
                            RecipientGroupId::To,
                            RecipientGroupId::Cc,
                            RecipientGroupId::Bcc,
                        ] {
                            let list = recipient_group_from_draft(&mut draft, id);
                            DraftOnRecipientValidation::new_list(
                                id,
                                actor_sender.clone(),
                                list,
                                cancellation_token.clone(),
                            )
                            .check_all(&ctx);
                        }
                    }
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                    publish_event(DraftEvent::RecipientListsUpdated {
                        to: draft.to_list.clone(),
                        cc: draft.cc_list.clone(),
                        bcc: draft.bcc_list.clone(),
                    });
                }

                DraftActorMessage::RemoveSingleRecipient {
                    group,
                    email,
                    sender,
                } => {
                    recipient_group_from_draft(&mut draft, group)
                        .remove_single(email.as_clear_text_str());
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                    let list = recipient_group_from_draft(&mut draft, group);
                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::RemoveGroupRecipient {
                    group,
                    email,
                    group_name,
                    sender,
                } => {
                    recipient_group_from_draft(&mut draft, group)
                        .remove_group_recipient(&group_name, email.as_clear_text_str());
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                    let list = recipient_group_from_draft(&mut draft, group);
                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::RemoveGroupRecipients {
                    group,
                    emails,
                    group_name,
                    sender,
                } => {
                    recipient_group_from_draft(&mut draft, group).remove_group_recipients(
                        &group_name,
                        emails.into_iter().map(|v| v.into_clear_text_string()),
                    );
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                    let list = recipient_group_from_draft(&mut draft, group);
                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::RemoveRecipientGroup {
                    group,
                    group_name,
                    sender,
                } => {
                    recipient_group_from_draft(&mut draft, group).remove_group(&group_name);
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let _ = sender.send(r);
                    let list = recipient_group_from_draft(&mut draft, group);
                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::GetRecipients { group, sender } => {
                    let recipients = recipient_group_from_draft(&mut draft, group)
                        .recipients()
                        .to_vec();
                    let _ = sender.send(recipients);
                }

                #[cfg(feature = "test-utils")]
                DraftActorMessage::TestMutate { mutate, sender } => {
                    mutate(&mut draft);
                    let _ = sender.send(());
                }

                DraftActorMessage::GetState(sender) => {
                    let state = DraftState::from_draft(&draft);
                    let _ = sender.send(state);
                }

                DraftActorMessage::SetSubject { subject, sender } => {
                    let r = if draft.subject != subject {
                        draft.subject = subject;
                        auto_saver.save(&ctx, &mut draft, &options).await
                    } else {
                        Ok(())
                    };
                    let _ = sender.send(r);
                }

                DraftActorMessage::GetSubject(sender) => {
                    let _ = sender.send(draft.subject.clone());
                }

                DraftActorMessage::GetRecipientList { group, sender } => {
                    let recipients = recipient_group_from_draft(&mut draft, group).clone();
                    let _ = sender.send(recipients);
                }

                DraftActorMessage::SetRecipientList {
                    group,
                    mut recipients,
                    sender,
                } => {
                    {
                        let list = recipient_group_from_draft(&mut draft, group);
                        std::mem::swap(list, &mut recipients);
                        if options.address_validation_enabled {
                            DraftOnRecipientValidation::new_list(
                                group,
                                actor_sender.clone(),
                                list,
                                cancellation_token.clone(),
                            )
                            .check_all(&ctx);
                        }
                    }
                    let r = auto_saver.save(&ctx, &mut draft, &options).await;
                    let list = recipient_group_from_draft(&mut draft, group);
                    if r.is_ok() {
                        publish_event(DraftEvent::RecipientListUpdated {
                            group,
                            list: list.clone(),
                        });
                    } else {
                        std::mem::swap(list, &mut recipients);
                    }
                    let _ = sender.send(r);
                }

                DraftActorMessage::OnRecipientValidation { group, updates } => {
                    let list = recipient_group_from_draft(&mut draft, group);
                    updates.apply(list);

                    publish_event(DraftEvent::RecipientListUpdated {
                        group,
                        list: list.clone(),
                    });
                }

                DraftActorMessage::RevalidateAllRecipients => {
                    if options.address_validation_enabled {
                        for id in [
                            RecipientGroupId::To,
                            RecipientGroupId::Cc,
                            RecipientGroupId::Bcc,
                        ] {
                            let list = recipient_group_from_draft(&mut draft, id);
                            DraftOnRecipientValidation::new_list(
                                id,
                                actor_sender.clone(),
                                list,
                                cancellation_token.clone(),
                            )
                            .check_all(&ctx);
                        }
                    }
                }

                DraftActorMessage::ValidateExpirationFeature(sender) => {
                    let r = async {
                        let tether = ctx.user_stash().connection().await?;

                        let metadata = DraftMetadata::find_by_id(draft.metadata_id, &tether)
                            .await?
                            .ok_or(Error::Expiration(ExpirationError::MetadataNotFound(
                                draft.metadata_id,
                            )))?;

                        let report = if metadata.expiration_time() == DraftExpirationTime::Never
                            || metadata.password.is_some()
                        {
                            // if we have password encryption or no expiration time is set, then
                            // every validly formatted email address is supported.
                            let mut report = ExpirationFeatureSupportReport::default();
                            draft
                                .to_list
                                .fill_expiration_support_report_as_supported(&mut report);
                            draft
                                .cc_list
                                .fill_expiration_support_report_as_supported(&mut report);
                            draft
                                .bcc_list
                                .fill_expiration_support_report_as_supported(&mut report);
                            report
                        } else {
                            let mut report = ExpirationFeatureSupportReport::default();
                            draft.to_list.validate_expiration_feature(&mut report);
                            draft.cc_list.validate_expiration_feature(&mut report);
                            draft.bcc_list.validate_expiration_feature(&mut report);
                            report
                        };

                        Ok(report)
                    }
                    .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SwapAttachmentDisposition {
                    attachment_id,
                    new_disposition,
                    sender,
                } => {
                    let r = draft
                        .swap_attachment_disposition(&ctx, attachment_id, new_disposition)
                        .await;
                    let _ = sender.send(r);
                }

                DraftActorMessage::SwapAttachmentDispositionCid { content_id, sender } => {
                    let r = draft
                        .swap_attachment_disposition_from_inline(&ctx, content_id)
                        .await;
                    let _ = sender.send(r);
                }
            }
        }

        tracing::info!("Terminating");
    }
}
fn recipient_group_from_draft(
    draft: &mut draft_v1::Draft,
    group: RecipientGroupId,
) -> &mut RecipientList {
    match group {
        RecipientGroupId::To => &mut draft.to_list,
        RecipientGroupId::Cc => &mut draft.cc_list,
        RecipientGroupId::Bcc => &mut draft.bcc_list,
    }
}

pub type Draft = DraftActor;

#[derive(Clone)]
struct DraftOnRecipientValidation {
    group: RecipientGroupId,
    sender: mpsc::Sender<DraftActorMessage>,
}

impl DraftOnRecipientValidation {
    fn new_list(
        group: RecipientGroupId,
        sender: mpsc::Sender<DraftActorMessage>,
        list: &mut RecipientList,
        cancellation_token: CancellationToken,
    ) -> DraftValidatingRecipientList<'_> {
        ValidatingRecipientList::new(
            cancellation_token,
            list,
            DraftOnRecipientValidation { group, sender },
        )
    }
}

impl OnBackgroundValidationComplete for DraftOnRecipientValidation {
    async fn recipients_validation_state_updated(&self, updates: RecipientValidationUpdate) {
        let _ = self
            .sender
            .send(DraftActorMessage::OnRecipientValidation {
                group: self.group,
                updates,
            })
            .await;
    }
}

type DraftValidatingRecipientList<'l> = ValidatingRecipientList<'l, DraftOnRecipientValidation>;

#[derive(Default)]
struct DraftAutoSaver {
    last_save_time: Option<Instant>,
    has_pending_saves: bool,
}

impl DraftAutoSaver {
    async fn map_save<T, E: Into<MailContextError>>(
        &mut self,
        result: Result<T, E>,
        ctx: &MailUserContext,
        draft: &mut draft_v1::Draft,
        options: &DraftActorOptions,
    ) -> Result<T, MailContextError> {
        let v = result.map_err(Into::into)?;
        self.save(ctx, draft, options).await?;
        Ok(v)
    }

    async fn save(
        &mut self,
        ctx: &MailUserContext,
        draft: &mut draft_v1::Draft,
        options: &DraftActorOptions,
    ) -> Result<(), MailContextError> {
        if !options.auto_save_enabled() {
            return Ok(());
        }
        self.do_save(ctx, draft).await
    }

    async fn periodic_save(
        &mut self,
        ctx: &MailUserContext,
        draft: &mut draft_v1::Draft,
        options: &DraftActorOptions,
    ) -> Result<(), MailContextError> {
        let Some(periodic_save_interval) = options.auto_save_every else {
            return Ok(());
        };

        if !self.should_auto_save(periodic_save_interval) {
            self.has_pending_saves = true;
            return Ok(());
        }

        self.do_save(ctx, draft).await
    }

    async fn do_save(
        &mut self,
        ctx: &MailUserContext,
        draft: &mut draft_v1::Draft,
    ) -> Result<(), MailContextError> {
        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection().await?;
        draft.save(queue, &tether, ctx.origin()).await?;
        self.reset_save_state();
        Ok(())
    }

    fn reset_save_state(&mut self) {
        self.last_save_time = Some(Instant::now());
        self.has_pending_saves = false;
    }

    fn should_auto_save(&self, periodic_save_interval: Duration) -> bool {
        let Some(last_save_time) = self.last_save_time else {
            return true;
        };

        let elapsed = Instant::now().duration_since(last_save_time);
        elapsed >= periodic_save_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_saver() {
        let mut saver = DraftAutoSaver::default();
        assert!(saver.should_auto_save(Duration::from_secs(10)));
        saver.reset_save_state();
        assert!(!saver.should_auto_save(Duration::from_secs(10)));
        assert!(saver.should_auto_save(Duration::from_nanos(1)));
    }
}
