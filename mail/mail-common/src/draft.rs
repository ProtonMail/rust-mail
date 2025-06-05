use crate::actions::draft;
use crate::actions::draft::{
    AttachmentRemove, AttachmentUpload, AttachmentUploadMode, Discard, Save, UndoSend,
};
use crate::datatypes::attachment::ContentId;
use crate::datatypes::{Disposition, LocalAttachmentId, LocalMessageId, MimeType};

use crate::decrypted_message::{DecryptedMessageBody, ThemeOpts};
use crate::draft::attachments::DraftAttachment;
use crate::draft::compose::{
    encrypt_draft_body, get_signature, inject_dark_mode, patch_draft_with_reply_mode,
    prepare_html_reply, prepare_plain_text_reply,
};
use crate::draft::recipients::{ContactGroupResolver, ProtonContactGroupResolver, RecipientList};
use crate::models::{
    Attachment, AttachmentType, DraftAttachmentMetadata, DraftAttachmentUploadState, DraftMetadata,
    DraftSendResult, DraftSendResultOrigin, EmbeddedAttachmentInfo, MailSettings, Message,
    MetadataId,
};
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use anyhow::Context;
use chrono::{DateTime, Local};
use compose::maybe_sanitize;
use derive_more::derive::TryFrom;
use futures::future::join3;
use proton_action_queue::action::{ActionId, MetadataBuilder};
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput, QueuedError};
use proton_core_api::consts::Mail;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_core_api::session::{CoreSession, Session};
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::attachment::{AttachmentDecryptionError, AttachmentEncryptionError};
use proton_crypto_inbox::keys::{PackageCryptoType, SessionKeyError};
use proton_crypto_inbox::message::MessageError;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::DraftReplyOrForwardParams;
use proton_mail_api::services::proton::request_data::{DraftAction, DraftAttachmentKeyPackets};
use proton_mail_api::services::proton::response_data::Message as ApiMessage;
use proton_mail_html_transformer::transforms::styles::BrowserCapabilities;
use proton_mail_ids::LocalConversationId;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, SqliteError, ToSql, ToSqlOutput};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, error};

pub mod attachments;
pub mod compose;
pub mod observers;
pub mod recipients;
pub(crate) mod send;

pub use send::ScheduleSendOptions;

/// Potential draft specific errors.
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
}

/// Errors that occur during draft creation or opening an existing draft.
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

/// Errors that occur when sending a draft.
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
    SechduleSendExpired,
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

/// Errors that occur while attempting to upload an attachment
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

/// Errors that occur while attempting to undo a sent message.
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

/// Errors that occur while discarding a draft.
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

/// Errors that occur while discarding a draft.
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

/// Potential draft specific errors.
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
    #[error("Failed to get attachment remote id")]
    AttachmentNoRemoteId,
    #[error("Failed to write mime body to buffer: {0}")]
    MimeBodyBuild(String),
    #[error("Failed to extract attachment info for address: {0}")]
    PackageBodyInfoReEncrypt(SessionKeyError),
    #[error("Failed to extract attachment info for address: {0}")]
    PackageAttachmentInfo(#[from] AttachmentDecryptionError),
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
    RecipientEmailInvalid(String),
    #[error("Proton Email {0} does not exist")]
    ProtonRecipientDoesNotExist(String),
}

/// Draft reply mode.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum ReplyMode {
    /// Reply only to the sender.
    Sender = 0,
    /// Reply to the sender and all recipients.
    All = 1,
    /// Forward the message.
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

/// Represent a new message that is being drafted.
///
/// When creating a new draft, empty or reply, we calculate what the
/// new draft should look like, but we never save it to disk until
/// the user calls [`save()`].
///
/// Since there is associated metadata with these operations, we create
/// a new [`DraftMetadata`] structure whenever we open or create a draft
/// so we can track auxiliary data such as the message id.
///
/// This metadata is kept alive as long as the message it references is alive
/// or the draft is discarded/deleted.
#[derive(derive_more::Debug)]
pub struct Draft {
    /// Id of the associated metadata.
    pub metadata_id: MetadataId,
    /// Sender email address
    pub sender: String,
    /// To Recipients addresses
    pub to_list: RecipientList,
    /// CC Recipients addresses
    pub cc_list: RecipientList,
    /// BCC recipients addresses
    pub bcc_list: RecipientList,
    /// Address used to send the message
    pub address_id: AddressId,
    /// Draft subject
    pub subject: String,
    /// `None` if there is no associated send result.
    pub send_result: Option<DraftSendResult>,
    #[debug(skip)]
    /// The decrypted message body.
    body: String,
    /// Message mime type
    mime_type: MimeType,
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

impl Draft {
    pub async fn schedule_send_options(
        ctx: &MailUserContext,
    ) -> MailContextResult<ScheduleSendOptions<Local>> {
        let user = ctx.user().await?;
        ScheduleSendOptions::new(user.subscribed)
            .context("Failed to get schedule send options")
            .map_err(MailContextError::Other)
    }
    /// Open an existing draft with `message_id` and load all the relevant information.
    ///
    /// # Errors
    ///
    /// Returns error if the draft failed to load, the message can't be found
    /// or the message is not a draft.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(context))]
    pub async fn open(
        context: &MailUserContext,
        message_id: LocalMessageId,
    ) -> Result<(Self, DraftSyncStatus), MailContextError> {
        let tether = &mut context.user_stash().connection();

        let Some(mut message) = Message::find_by_id(message_id, tether).await? else {
            error!("Opened message as draft that does not exist.");
            return Err(AppError::MessageMissing(message_id).into());
        };

        // Ignore deleted messages.
        if message.deleted {
            return Err(AppError::MessageMissing(message_id).into());
        }

        if !message.is_draft() {
            error!("Opened a non-draft message as a draft");
            return Err(OpenError::MessageNotADraft(message_id).into());
        }

        let metadata = if let Some(metadata) =
            DraftMetadata::find_by_message_id(message.local_id.unwrap(), tether)
                .await
                .inspect_err(|e| error!("Failed to load draft metadata: {e:?}"))?
        {
            debug!("Found existing metadata with id {}", metadata.id.unwrap());
            metadata
        } else {
            debug!("No metadata found, creating new entry");
            let mut metadata = DraftMetadata {
                id: None,
                local_message_id: Some(message.local_id.unwrap()),
                local_conversation_id: Some(message.local_conversation_id.unwrap()),
                local_parent_id: None,
                reply_mode: None,
                send_action_id: None,
                save_action_id: None,
                row_id: None,
            };
            tether
                .tx::<_, _, MailContextError>(async |tx| {
                    metadata
                        .save(tx)
                        .await
                        .inspect_err(|e| error!("Failed to create new metadata: {e:?}"))?;
                    tokio::fs::create_dir_all(draft_attachment_staging_path(
                        context,
                        metadata.id.unwrap(),
                    ))
                    .await
                    .inspect_err(|e| error!("Failed to create attachment staging path: {e:?}"))?;
                    Ok(())
                })
                .await?;
            metadata
        };

        // First let's try to sync the body and metadata. If we can't we will fill it
        // ourselves.
        let (decrypted, sync_status) = if metadata.has_pending_changes(tether).await? {
            // If we have pending changes we should not sync the data from the server
            // as that will override local state.
            debug!("Draft metadata has pending changes, sync skipped.");
            (None, DraftSyncStatus::Synced)
        } else if let Some(remote_id) = message.remote_id.clone() {
            debug!("Draft metadata has no pending changes, syncing.");
            match Message::force_sync_message_and_body(context, remote_id, true).await {
                Ok((message_new, decrypted)) => {
                    message = message_new;

                    debug!("Message synced, updating attachment metadata.");
                    tether
                        .tx(async |tx| {
                            DraftAttachmentMetadata::reset_draft_attachments_after_sync(
                                metadata.id.unwrap(),
                                &decrypted.metadata,
                                tx,
                            )
                            .await
                        })
                        .await?;

                    (Some(decrypted), DraftSyncStatus::Synced)
                }
                // Handle network failure
                Err(MailContextError::Api(api_err)) if api_err.is_network_failure() => {
                    debug!("Failed to sync draft due to network error.");
                    (None, DraftSyncStatus::Cached)
                }
                Err(e) => return Err(e),
            }
        } else {
            debug!("Message does not have a remote id.");
            // If we have no remote id do not return cached status. As this implies the
            // draft was created locally and the save action has not yet executed.
            // We only trigger this code path if the save action failed to execute.
            (None, DraftSyncStatus::Synced)
        };

        let decrypted = match decrypted {
            Some(d) => d,
            None => {
                debug!("Failed to sync draft from server, attempting to load from cache.");
                let Some(d) =
                    Message::load_decrypted_message_from_cache(message.local_id.unwrap(), tether)
                        .await
                        .inspect_err(|e| {
                            error!("Failed to load decrypted data from cache: {e:?}")
                        })?
                else {
                    return Err(OpenError::MessageBodyMissing(message.local_id.unwrap()).into());
                };
                d
            }
        };

        let send_result = DraftSendResult::find_by_id(message.local_id.unwrap(), tether)
            .await
            .inspect_err(|e| error!("Failed to load send result: {e:?}"))?;

        let contact_group_resolver = ProtonContactGroupResolver::new(tether);
        let (to_list, cc_list, bcc_list) = join3(
            RecipientList::from_message_recipients(&contact_group_resolver, message.to_list.value),
            RecipientList::from_message_recipients(&contact_group_resolver, message.cc_list.value),
            RecipientList::from_message_recipients(&contact_group_resolver, message.bcc_list.value),
        )
        .await;

        let mut draft = Self {
            metadata_id: metadata.id.unwrap(),
            sender: message.sender.address,
            to_list,
            cc_list,
            bcc_list,
            address_id: message.remote_address_id,
            subject: message.subject,
            send_result,
            body: decrypted.body,
            mime_type: decrypted.metadata.mime_type,
        };
        draft.sanitize_body();

        Ok((draft, sync_status))
    }

    /// Create a new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip_all)]
    pub async fn empty(context: &MailUserContext) -> Result<Self, MailContextError> {
        let mut tether = context.user_stash().connection();
        // Default address should have display_order 0
        let addresses = Address::find("ORDER BY display_order ASC LIMIT 1", vec![], &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load addresses: {e:?}");
            })?;

        if addresses.is_empty() {
            error!("No addresses found for current user");
            return Err(OpenError::UserHasNoAddresses.into());
        }

        let address = &addresses[0];
        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();

        let metadata = tether
            .tx::<_, _, MailContextError>(async |tx| {
                let metadata = DraftMetadata::empty(tx)
                    .await
                    .inspect_err(|e| error!("Failed to create new empty draft metadata: {e:?}"))?;
                if mail_settings.attach_public_key {
                    let public_key_attachment = Attachment::create_public_key(context, address, tx)
                        .await
                        .inspect_err(|e| error!("Failed to create public key attachment: {e:?}"))?;

                    DraftAttachmentMetadata::pending(
                        metadata.id.unwrap(),
                        public_key_attachment.local_id.unwrap(),
                        0,
                        true,
                    )
                    .save(tx)
                    .await?
                }
                Ok(metadata)
            })
            .await?;

        Ok(Self::new_empty_draft(
            metadata.id.unwrap(),
            address,
            &mail_settings,
        ))
    }

    /// Create new empty draft from `address`.
    ///
    /// Note: This is split up from [`Self::empty()`] for testing.
    fn new_empty_draft(
        metadata_id: MetadataId,
        address: &Address,
        mail_settings: &MailSettings,
    ) -> Self {
        let body = compose::get_signature(address, mail_settings, mail_settings.draft_mime_type);
        Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            send_result: None,
            mime_type: mail_settings.draft_mime_type,
            body,
        }
    }

    /// Create a draft as reply/forward to an existing message with `message_id`.
    ///
    /// `use_utc` controls whether we should generate the sender reply using
    /// the `Utc` or `Local` timezone. For production, we should use the `Local`
    /// but for testing in CI `Utc` is more deterministic.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(context))]
    pub async fn reply(
        context: &MailUserContext,
        message_id: LocalMessageId,
        reply_mode: ReplyMode,
        use_utc: bool,
        mime_type_override: Option<MimeType>,
    ) -> Result<Self, MailContextError> {
        let mut tether = context.user_stash().connection();
        // Load the message we reply to.
        let Some(source_message) = Message::find_by_id(message_id, &tether).await? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        // Source message can not be a draft.
        if source_message.flags.is_draft() {
            return Err(OpenError::ReplyOrForwardToDraft(message_id).into());
        }

        // Source message much have a remote id.
        if source_message.remote_id.is_none() {
            return Err(AppError::MessageHasNoRemoteId(message_id).into());
        }

        // Find out which address this message has and use that to craft te reply.
        let Some(address) =
            Address::find_by_remote_id(source_message.remote_address_id.clone(), &tether).await?
        else {
            return Err(
                OpenError::AddressNotFound(source_message.remote_address_id.clone()).into(),
            );
        };

        // Message body must be present to create a reply.
        let Some(source_message_body) =
            Message::load_decrypted_message_from_cache(message_id, &tether)
                .await
                .inspect_err(|e| error!("Failed to get source decrypted message: {e:?}"))?
        else {
            return Err(OpenError::MessageBodyMissing(message_id).into());
        };

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        tether
            .tx(async |tx| {
                let metadata = DraftMetadata::reply(
                    reply_mode,
                    source_message.local_id.unwrap(),
                    source_message.local_conversation_id.unwrap(),
                    tx,
                )
                .await
                .inspect_err(|e| error!("Failed to create new reply draft metadata: {e:?}"))?;

                tokio::fs::create_dir_all(draft_attachment_staging_path(
                    context,
                    metadata.id.unwrap(),
                ))
                .await
                .inspect_err(|e| error!("Failed to create attachment staging path: {e:?}"))?;

                let contact_group_resolver = ProtonContactGroupResolver::new(tx);

                let (draft, attachments) = Self::new_draft_reply(
                    &contact_group_resolver,
                    metadata.id.unwrap(),
                    reply_mode,
                    &address,
                    &mail_settings,
                    &source_message,
                    source_message_body,
                    use_utc,
                    mime_type_override,
                )
                .await;

                if mail_settings.attach_public_key {
                    let public_key_attachment =
                        Attachment::gen_public_key(context, &address, tx).await?;

                    // If we already have the public key, we should just skip adding the attachment.
                    if !attachments.iter().any(|attachment| {
                        attachment.filename == public_key_attachment.attachment.filename
                    }) {
                        let attachment = public_key_attachment.store(context, tx).await?;
                        DraftAttachmentMetadata::pending(
                            metadata.id.unwrap(),
                            attachment.local_id.unwrap(),
                            0,
                            true,
                        )
                        .save(tx)
                        .await?;
                    }
                }

                for (order, attachment) in attachments.into_iter().enumerate() {
                    let mut attachment_metadata =
                        if matches!(attachment.attachment_type, AttachmentType::Pgp) {
                            // PGP attachments need to be cloned and uploaded to the server so it can be sent.
                            debug!("Cloning PGP attachment {} ", attachment.local_id.unwrap());
                            let new_attachment = Attachment::clone_attachment(
                                context,
                                address.remote_id.clone().unwrap(),
                                attachment,
                                tx,
                            )
                            .await
                            .inspect_err(|e| error!("Failed to clone pgp attachment: {e:?}",))?;
                            debug!(
                                "PGP attachment cloned as {} ",
                                new_attachment.local_id.unwrap()
                            );
                            DraftAttachmentMetadata::pending(
                                metadata.id.unwrap(),
                                new_attachment.local_id.unwrap(),
                                order,
                                false,
                            )
                        } else {
                            DraftAttachmentMetadata::inherited(
                                metadata.id.unwrap(),
                                &attachment,
                                order,
                            )
                        };

                    attachment_metadata
                        .save(tx)
                        .await
                        .inspect_err(|e| error!("Failed to save attachment metadata: {e:?}"))?
                }
                Ok(draft)
            })
            .await
    }

    /// Create a draft reply.
    ///
    /// # Params
    ///
    /// `metadata_id`          - Metadata id for this draft.
    /// `reply_mode`           - Draft reply mode.
    /// `address`              - Sender address.
    /// `source_message`       - Metadata of the message we are replying to.
    /// `source_message_body`  - Body of the message we are replying to.
    /// `use_utc`              - Whether to use utc over local timezone.
    /// `session_id`           - Id of the current network session.
    ///
    /// Note: This function is separate so it is easier to test.
    #[allow(clippy::too_many_arguments)]
    async fn new_draft_reply(
        contact_group_resolver: &impl ContactGroupResolver,
        metadata_id: MetadataId,
        reply_mode: ReplyMode,
        address: &Address,
        mail_settings: &MailSettings,
        source_message: &Message,
        source_message_body: DecryptedMessageBody,
        use_utc: bool,
        mime_type_override: Option<MimeType>,
    ) -> (Self, Vec<Attachment>) {
        let mime_type = if let Some(mime_type) = mime_type_override {
            mime_type
        } else if mail_settings.draft_mime_type == MimeType::TextHtml
            || source_message_body.metadata.mime_type == MimeType::TextHtml
        {
            MimeType::TextHtml
        } else {
            MimeType::TextPlain
        };

        let mut body = get_signature(address, mail_settings, mime_type);

        // If the message we are replying to is HTML we should also generate an HTML body for
        // replying even if the user has selected plain text as the default editing mode.
        if mime_type == MimeType::TextHtml {
            prepare_html_reply(
                &mut body,
                source_message,
                &source_message_body.body,
                use_utc,
            );
        } else {
            prepare_plain_text_reply(
                &mut body,
                source_message,
                &source_message_body.body,
                source_message_body.metadata.mime_type,
                use_utc,
            );
        };

        let mut attachments = source_message_body.metadata.attachments;

        if reply_mode != ReplyMode::Forward {
            attachments.retain(|attachment| attachment.disposition == Disposition::Inline);
        };

        let mut draft = Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            send_result: None,
            body,
            mime_type,
        };

        patch_draft_with_reply_mode(
            contact_group_resolver,
            &mut draft,
            source_message,
            reply_mode,
            address,
        )
        .await;

        draft.sanitize_body();

        (draft, attachments)
    }

    /// Create new draft on the server
    ///
    /// # Params
    ///
    /// * `context`                : Mail user context to access the cache and crypto keys.
    /// * `session`                : Networks session
    /// * `address_id`             : Address id to with witch to encrypt the message.
    /// * `message`                : Message metadata form which to create a draft.
    /// * `message_body_metadata`  : Message body metadata from which to create a draft.
    /// * `message_body`           : Body of the draft
    ///
    /// # Errors
    ///
    /// Returns an error if the request failed or if the body could not be
    /// encrypted.
    #[allow(clippy::too_many_arguments)]
    pub async fn remote_create(
        context: &MailUserContext,
        session: &Session,
        address_id: AddressId,
        save_action: &Save,
        attachments: &[Attachment],
        message_body: &str,
        draft_reply_or_forward_params: Option<DraftReplyOrForwardParams>,
    ) -> Result<ApiMessage, MailContextError> {
        let encrypted = encrypt_draft_body(context, &address_id, message_body).await?;
        let params = save_action.crate_draft_params(encrypted);

        let mut attachment_key_packets = DraftAttachmentKeyPackets::new();
        debug!("Draft create with {} attachments", attachments.len());
        for attachment in attachments {
            let Some(remote_id) = attachment.remote_id().clone() else {
                // When adding new attachment to a draft, we reflect the state correctly offline
                // but we can not attach an attachment until it has a remote id. We skip attachments
                // that still does not have a remote id. Since we always save before send and send
                // also requires all attachments to be uploaded this will correct itself.
                tracing::warn!(
                    "Attachment {} does not have a remote id, skipping",
                    attachment.local_id.unwrap()
                );
                continue;
            };
            let Some(key_packets) = attachment.key_packets.clone() else {
                return Err(SaveError::AttachmentDoesNotHaveKeyPackets(
                    attachment.local_id.unwrap(),
                )
                .into());
            };
            attachment_key_packets.insert(remote_id, key_packets.value.clone());
        }

        let response = session
            .api()
            .create_draft(
                params,
                attachment_key_packets,
                draft_reply_or_forward_params,
            )
            .await?;
        Ok(response.message)
    }

    /// Update an existing draft on the server
    ///
    /// # Params
    ///
    /// * `context`                : Mail user context to access the cache and crypto keys.
    /// * `session`                : Networks session
    /// * `address_id`             : Address id to with witch to encrypt the message.
    /// * `message`                : Message metadata form which to create a draft.
    /// * `message_body_metadata`  : Message body metadata from which to create a draft.
    /// * `message_body`           : Body of the draft
    ///
    /// # Errors
    ///
    /// Returns an error if the request failed or if the body could not be
    /// encrypted.
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
    ) -> Result<ApiMessage, MailContextError> {
        let encrypted = encrypt_draft_body(context, &address_id, message_body).await?;
        let params = save_action.crate_draft_params(encrypted);

        let mut attachment_key_packets = DraftAttachmentKeyPackets::new();
        debug!("Draft update with {} attachments", attachments.len());
        for attachment in attachments {
            let Some(remote_id) = attachment.remote_id().clone() else {
                // When adding new attachment to a draft, we reflect the state correctly offline
                // but we can not attach an attachment until it has a remote id. We skip attachments
                // that still does not have a remote id. Since we always save before send and send
                // also requires all attachments to be uploaded this will correct itself.
                tracing::warn!(
                    "Attachment {} does not have a remote id, skipping",
                    attachment.local_id.unwrap()
                );
                continue;
            };
            let Some(key_packets) = attachment.key_packets.clone() else {
                return Err(SaveError::AttachmentDoesNotHaveKeyPackets(
                    attachment.local_id.unwrap(),
                )
                .into());
            };
            attachment_key_packets.insert(remote_id, key_packets.value.clone());
        }

        match session
            .api()
            .update_draft(message_id, params, attachment_key_packets)
            .await
        {
            Err(e) => {
                if let Some(proton_error) = e.to_proton_error() {
                    if proton_error.code == Mail::MessageAlreadySent as u32 {
                        return Err(SaveError::AlreadySent.into());
                    } else if proton_error.code == Mail::MessageUpdateDraftNotDraft as u32 {
                        return Err(SaveError::MessageNotADraft(local_message_id).into());
                    } else if proton_error.code == Mail::MessageUpdateDraftNotExist as u32 {
                        return Err(SaveError::DraftDoesNotExistOnServer.into());
                    }
                }
                Err(e.into())
            }
            Ok(response) => Ok(response.message),
        }
    }

    /// Apply an action which will create a new draft.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self,queue))]
    pub async fn save(
        &mut self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<Save>, MailContextError> {
        let queued_output = self.to_save_action().queue(queue, tether).await?;
        Ok(queued_output)
    }

    /// Apply an action which will send this draft.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self,queue))]
    pub async fn send(
        &mut self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.to_send_action()?.queue(queue, tether).await
    }

    /// Apply an action which will schedule a send this draft at the given `delivery_time`.
    ///
    /// Note that due to offline mode we will only send this message if at the time we are
    /// executing the request, there is still enough time left to schedule the send.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self,queue))]
    pub async fn schedule_send(
        &mut self,
        delivery_time: DateTime<Local>,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.to_schedule_send_action(delivery_time)?
            .queue(queue, tether)
            .await
    }

    /// Discard the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(self,queue))]
    pub async fn discard(
        &self,
        queue: &Queue,
    ) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        Ok(self.to_discard_action().queue(queue).await?)
    }

    /// Discard a draft with the given `message_id`.
    ///
    /// This is functionally equivalent to [`Draft::discard()`] but does not
    /// require an instance of the [`Draft`] type.
    ///
    /// # Remarks
    ///
    /// This still requires that this message has been opened with `Draft::open` at least
    /// once.
    ///
    /// # Errors
    ///
    /// Returns error if the message is not a draft or the action failed to execute.
    pub async fn action_discard(
        message_id: LocalMessageId,
        tether: &Tether,
        queue: &Queue,
    ) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        let Some(metadata) = DraftMetadata::find_by_message_id(message_id, tether).await? else {
            return Err(Error::Open(OpenError::MessageNotADraft(message_id)).into());
        };

        Ok(
            DraftDiscardActionQueuer::new(metadata.id.unwrap(), Discard::new(metadata.id.unwrap()))
                .queue(queue)
                .await?,
        )
    }

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    ///
    /// While we have our own instance variable for the `last_save_action_id`, it may
    /// be beneficial for users of this method to pass in an alternate source.
    ///
    pub fn to_save_action(&self) -> DraftSaveActionQueuer {
        DraftSaveActionQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            Save::new(self, DraftSendResultOrigin::Save),
        )
    }

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    /// While we have our own instance variable for the `last_save_action_id`, it may
    /// be beneficial for users of this method to pass in an alternate source.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub fn to_send_action(&self) -> Result<DraftSendActionQueuer, Error> {
        self.to_send_action_impl(None)
    }

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    /// While we have our own instance variable for the `last_save_action_id`, it may
    /// be beneficial for users of this method to pass in an alternate source.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub fn to_schedule_send_action(
        &self,
        delivery_time: DateTime<Local>,
    ) -> Result<DraftSendActionQueuer, Error> {
        self.to_send_action_impl(Some(delivery_time))
    }

    fn to_send_action_impl(
        &self,
        delivery_time: Option<DateTime<Local>>,
    ) -> Result<DraftSendActionQueuer, Error> {
        if self.to_list.is_empty() && self.cc_list.is_empty() && self.bcc_list.is_empty() {
            return Err(SendError::NoRecipients.into());
        }
        let save_action = Save::new(self, DraftSendResultOrigin::SaveBeforeSend);
        let send_action = if let Some(delivery_time) = delivery_time {
            draft::Send::scheduled(self, delivery_time)
        } else {
            draft::Send::new(self)
        };
        let metadata_id = self.metadata_id;
        Ok(DraftSendActionQueuer::new(
            metadata_id,
            save_action,
            send_action,
        ))
    }

    /// Create a discard action for the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    pub fn to_discard_action(&self) -> DraftDiscardActionQueuer {
        DraftDiscardActionQueuer::new(self.metadata_id, Discard::new(self.metadata_id))
    }

    /// Get the message id associated with this draft.
    ///
    /// This method can return `None` if the message has not been
    /// created yet.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn message_id(&self, tether: &Tether) -> Result<Option<LocalMessageId>, StashError> {
        DraftMetadata::message_id(self.metadata_id, tether).await
    }

    /// Get the conversation id associated with this draft.
    ///
    /// This method can return `None` if the draft is a new empty reply
    /// and the conversation has not yet been created.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn conversation_id(
        &self,
        tether: &Tether,
    ) -> Result<Option<LocalConversationId>, StashError> {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, tether).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_conversation_id)
    }

    /// Enqueue a new cancel send action for the message with `message_id`
    ///
    /// # Errors
    ///
    /// Returns error if the message can't be undo sent or local operations failed.
    pub async fn action_undo_send(
        queue: &Queue,
        message_id: LocalMessageId,
    ) -> Result<QueuedActionOutput<UndoSend>, ActionError<UndoSend>> {
        queue.queue_action(UndoSend::new(message_id)).await
    }

    /// Load an embedded attachment in this draft message.
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    ///
    /// # Errors
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    pub async fn get_embedded_attachment(
        &self,
        ctx: &MailUserContext,
        cid: &ContentId,
    ) -> MailContextResult<EmbeddedAttachmentInfo> {
        let mut tether = ctx.user_stash().connection();
        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(self.metadata_id, &tether).await?;
        if let Some(attachment) = attachments
            .iter()
            .find(|a| a.content_id.as_ref() == Some(cid))
        {
            let data = attachment.content_data(ctx, &mut tether).await?;
            Ok(EmbeddedAttachmentInfo {
                data,
                mime: attachment.mime_type.to_string(),
                height: attachment.image_height.clone(),
                width: attachment.image_width.clone(),
            })
        } else {
            Err(AppError::UnknownCid(cid.clone(), vec![]).into())
        }
    }

    /// Delete an attachment file, but only if it is part of the draft staging area.
    ///
    /// If the removal fails, due to file locks, it will be GCed later by a background task.
    ///
    /// # Errors
    ///
    /// Returns error if the remove failed.
    pub async fn delete_attachment_if_in_staging_area(&self, ctx: &MailUserContext, path: &Path) {
        let staging_path = self.attachment_staging_path(ctx);
        if path.starts_with(&staging_path) {
            if let Err(e) = tokio::fs::remove_file(&staging_path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    // This is a warning as the background process will try again.
                    tracing::warn!(
                        "Failed to delete attachment from staging area at {path:?}: {e:?}"
                    );
                }
            }
        }
    }

    /// Add a new `attachment` to this draft.
    ///
    /// Use [`Attachment::create_local`] to create a new attachment first.
    ///
    /// # Errors
    ///
    /// Returns error if the query or queuing the action failed.
    pub async fn add_attachment(
        &self,
        ctx: &MailUserContext,
        attachment: Attachment,
    ) -> Result<ActionId, MailContextError> {
        let upload_action = self.to_add_attachment_action(attachment);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection();
        let result = upload_action.queue(queue, &tether).await?;

        Ok(result.id)
    }

    /// Add a new `attachment` to this draft.
    ///
    /// Similar to [`add_attachment`] but return an action queuer instead.
    ///
    pub fn to_add_attachment_action(&self, attachment: Attachment) -> DraftAttachmentUploadQueuer {
        // create save action before the attachment is registered as we need a message to upload.
        let save_action = self.to_save_action();
        let attachment_id = attachment.local_id.unwrap();

        DraftAttachmentUploadQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            attachment_id,
            save_action,
            AttachmentUploadMode::Create,
        )
    }

    /// Remove the attachment with `attachment_id` from  this draft.
    ///
    /// Use [`Attachment::create_local`] to create a new attachment first.
    ///
    /// # Errors
    ///
    /// Returns error if the query or queuing the action failed.
    pub async fn remove_attachment(
        &self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        let remove_action = self.to_remove_attachment_action(attachment_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection();
        let result = remove_action.queue(queue, &tether).await?;

        Ok(result.id)
    }

    /// Remove the attachment with `attachment_id` from  this draft.
    ///
    /// Similar to [`remove_attachment`] but return an action queuer instead.
    ///
    pub fn to_remove_attachment_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> DraftAttachmentRemovalQueuer {
        DraftAttachmentRemovalQueuer::new(
            self.metadata_id,
            AttachmentRemovalId::Local(attachment_id),
        )
    }

    /// Remove the attachment with `content_id` from  this draft.
    ///
    /// Use [`Attachment::create_local`] to create a new attachment first.
    ///
    /// # Errors
    ///
    /// Returns error if the query or queuing the action failed.
    pub async fn remove_attachment_with_cid(
        &self,
        ctx: &MailUserContext,
        content_id: ContentId,
    ) -> Result<ActionId, MailContextError> {
        let remove_action = self.to_remove_attachment_action_with_cid(content_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection();
        let result = remove_action.queue(queue, &tether).await?;

        Ok(result.id)
    }

    /// Remove the attachment with `content_id` from  this draft.
    ///
    /// Similar to [`remove_attachment_with_cid`] but return an action queuer instead.
    ///
    pub fn to_remove_attachment_action_with_cid(
        &self,
        content_id: ContentId,
    ) -> DraftAttachmentRemovalQueuer {
        DraftAttachmentRemovalQueuer::new(self.metadata_id, AttachmentRemovalId::Cid(content_id))
    }

    /// Retry the upload of a failed attachment.
    ///
    /// # Errors
    ///
    /// Returns error if the attachment is not in the error state or the action could not
    /// be queued.
    pub async fn retry_attachment_upload(
        &self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        let upload_action = self.to_retry_attachment_upload_action(attachment_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection();
        let result = upload_action.queue(queue, &tether).await?;
        Ok(result.id)
    }

    /// Create action queuer where the attachment upload is retried.
    ///
    /// It will only be accepted if the state is [`DraftAttachmentUploadState::Error`]
    pub fn to_retry_attachment_upload_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> DraftAttachmentUploadQueuer {
        let save_action = self.to_save_action();
        DraftAttachmentUploadQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            attachment_id,
            save_action,
            AttachmentUploadMode::Retry,
        )
    }

    /// Get the path where attachments should be staged.
    pub fn attachment_staging_path(&self, context: &MailUserContext) -> PathBuf {
        draft_attachment_staging_path(context, self.metadata_id)
    }

    /// Get the list of attachments and their upload status.
    pub async fn attachments(&self, tether: &Tether) -> Result<Vec<DraftAttachment>, StashError> {
        DraftAttachment::build_list(self.metadata_id, tether).await
    }

    /// On-the-fly generated head with injected the dark mode styles.
    /// The content of returned string depends on body and modifies it.
    ///
    /// # Parameters
    ///
    /// * `editor_id` - the HTML ID of the editor that wraps the message. The same used to reference DOM in javascript.
    ///
    /// # Modifications to the body
    ///
    /// * If the body contains `!important` flag, it will be removed.
    ///
    /// # Returned HTML
    ///
    /// This function returns HTML that can be inserted INTO `<head>` tag.
    /// It does not provide `<head>` tag on its own.
    /// Therefore, the returned HTML can be inserted alongside with other html nodes.
    ///
    /// ## Example of usage
    ///
    /// ```ignore
    /// let head_to_inject = draft.html_head_content_for_composer(theme_opts);
    ///
    /// let template = format!("
    /// <html>
    /// <head>
    ///
    ///    <meta ...things set up for the composer />
    ///
    ///    {head_to_inject}
    ///
    /// </head>
    /// <body>
    /// ...
    /// </body>
    /// </html>
    /// ");
    ///
    /// ```
    pub fn html_head_content_for_composer(
        &mut self,
        theme_opts: ThemeOpts,
        editor_id: String,
    ) -> String {
        let color_mode = theme_opts.color_mode();

        let mime_type = self.mime_type();

        let injection = inject_dark_mode(
            mime_type,
            &self.body,
            color_mode,
            BrowserCapabilities {
                supports_dark_mode_via_media_query: theme_opts.supports_dark_mode_via_media_query,
            },
            editor_id,
        );
        self.body = injection.body;

        injection.head
    }

    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn body_mut(&mut self) -> &mut String {
        &mut self.body
    }

    pub fn set_body(&mut self, body: String) {
        self.body = body;
    }

    pub async fn attachments_compat(&self, tether: &Tether) -> Result<Vec<Attachment>, StashError> {
        DraftAttachmentMetadata::attachment_for_draft(self.metadata_id, tether).await
    }

    pub fn mime_type(&self) -> MimeType {
        self.mime_type
    }

    pub fn set_mime_type(&mut self, mime_type: MimeType) {
        self.mime_type = mime_type;
    }

    pub fn sanitize_body(&mut self) {
        self.body = maybe_sanitize(self.mime_type(), &self.body);
    }

    pub async fn cancel_schedule_send(
        ctx: &MailUserContext,
        message_id: LocalMessageId,
    ) -> MailContextResult<DateTime<Local>> {
        let mut tether = ctx.user_stash().connection();
        let queue = ctx.action_queue();
        let timeout = Duration::from_secs(15);
        let session = ctx.session();
        send::cancel_schedule_send(message_id, &mut tether, queue, session, timeout).await
    }
}

/// Utility type to disconnect queueing of the action from the [`Draft`] type in multithreaded
/// context.
pub struct DraftSaveActionQueuer {
    id: MetadataId,
    address_id: AddressId,
    action: Save,
}

impl DraftSaveActionQueuer {
    fn new(id: MetadataId, address_id: AddressId, action: Save) -> Self {
        Self {
            id,
            address_id,
            action,
        }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::save",skip(self,queue))]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<Save>, MailContextError> {
        // find all attachments that need to be manually queued.
        let pending_attachment_ids =
            DraftAttachmentMetadata::pending_attachments(self.id, tether).await?;
        // We need to be aware of the last save action id to try and replace the existing one.
        // On failure, we only execute after the previous one has finished,
        let last_draft_save_action_id = DraftMetadata::last_save_action_id(self.id, tether).await?;
        let output =
            queue_or_replace_draft_save(queue, self.action, self.id, last_draft_save_action_id, [])
                .await?;

        //TODO: queue batching so we can fail everything together.
        for attachment_id in pending_attachment_ids {
            tracing::info!("Queuing attachment upload for pending attachment {attachment_id}");
            let metadata = MetadataBuilder::new()
                .with_resource(&self.id)
                .expect("This should never fail")
                .with_dependency(output.id)
                .build();
            queue
                .queue_action_with_metadata(
                    AttachmentUpload::new(
                        self.id,
                        self.address_id.clone(),
                        attachment_id,
                        AttachmentUploadMode::Create,
                    ),
                    metadata,
                )
                .await?;
        }

        Ok(output)
    }
}

/// Utility type to disconnect queueing of the action from the [`Draft`] type in multithreaded
/// context.
pub struct DraftSendActionQueuer {
    id: MetadataId,
    save_action: Save,
    send_action: draft::Send,
}

impl DraftSendActionQueuer {
    fn new(id: MetadataId, save_action: Save, send_action: draft::Send) -> Self {
        Self {
            id,
            save_action,
            send_action,
        }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::send",skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        let attachment_action_ids =
            DraftAttachmentMetadata::find_attachment_upload_action_ids(self.id, tether).await?;
        let last_draft_save_action_id = DraftMetadata::last_save_action_id(self.id, tether).await?;
        let save_output = queue_or_replace_draft_save(
            queue,
            self.save_action,
            self.id,
            last_draft_save_action_id,
            attachment_action_ids,
        )
        .await?;
        let send_metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail")
            .with_dependency(save_output.id)
            .build();
        Ok(queue
            .queue_action_with_metadata(self.send_action, send_metadata)
            .await?)
    }
}

/// Utility type to disconnect queueing of the action from the [`Draft`] type in multithreaded
/// context.
pub struct DraftDiscardActionQueuer {
    id: MetadataId,
    action: Discard,
}

impl DraftDiscardActionQueuer {
    fn new(id: MetadataId, action: Discard) -> Self {
        Self { id, action }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::discard",skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
    ) -> Result<QueuedActionOutput<Discard>, ActionError<Discard>> {
        queue
            .queue_action_with_metadata(
                self.action,
                MetadataBuilder::new()
                    .with_resource(&self.id)
                    .expect("This should never fail")
                    .build(),
            )
            .await
    }
}

/// Utility type to wrap the queueing of attachments upload.
///
/// We need to make sure that at least one save action is run before this action as we need
/// a remote id to upload.
pub struct DraftAttachmentUploadQueuer {
    id: MetadataId,
    attachment_id: LocalAttachmentId,
    address_id: AddressId,
    save_action: DraftSaveActionQueuer,
    mode: AttachmentUploadMode,
}

impl DraftAttachmentUploadQueuer {
    fn new(
        id: MetadataId,
        address_id: AddressId,
        attachment_id: LocalAttachmentId,
        save_action: DraftSaveActionQueuer,
        mode: AttachmentUploadMode,
    ) -> Self {
        Self {
            id,
            address_id,
            attachment_id,
            save_action,
            mode,
        }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::attachment_upload",skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<AttachmentUpload>, MailContextError> {
        let mut last_draft_save_action_id = None;
        // We only need this when creating, if we are retrying this must have already
        // happened.
        if self.mode == AttachmentUploadMode::Create {
            let message_has_remote_id =
                if let Some(local_message_id) = DraftMetadata::message_id(self.id, tether).await? {
                    Message::local_id_counterpart(local_message_id, tether)
                        .await?
                        .is_some()
                } else {
                    false
                };

            // We only want to issue a save action if the draft does not yet have a remote id, otherwise
            // we can't upload the attachment.
            if !message_has_remote_id {
                // If an existing save is ongoing, we want to depend on that action first, otherwise
                // we create a new one ourselves.
                last_draft_save_action_id =
                    DraftMetadata::last_save_action_id(self.id, tether).await?;
                if last_draft_save_action_id.is_none() {
                    last_draft_save_action_id =
                        Some(self.save_action.queue(queue, tether).await?.id)
                };
            };
        }

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail");
        if let Some(last_draft_save_action_id) = last_draft_save_action_id {
            metadata = metadata.with_dependency(last_draft_save_action_id)
        }

        // If we are retrying we should wait for the existing one to
        if self.mode == AttachmentUploadMode::Retry {
            let Some(attachment_metadata) =
                DraftAttachmentMetadata::find_by_id(self.attachment_id, tether).await?
            else {
                return Err(
                    AttachmentUploadError::AttachmentDataMissing(self.attachment_id).into(),
                );
            };

            // If the state is not error, we should not allow the retry.
            if attachment_metadata.state() != DraftAttachmentUploadState::Error {
                error!(
                    "Attempting attachment ({}) upload retry on non error state",
                    self.attachment_id
                );
                return Err(AttachmentUploadError::RetryInvalidState(self.attachment_id).into());
            }

            // In case there is still an action, we only want to run after that. Action id is
            // cleaned up on cancel and failure, but due to scheduling it's possible this value
            // is still around.
            if let Some(action_id) = attachment_metadata.action_id {
                metadata = metadata.with_dependency(action_id);
            }
        }

        let metadata = metadata.build();
        Ok(queue
            .queue_action_with_metadata(
                AttachmentUpload::new(self.id, self.address_id, self.attachment_id, self.mode),
                metadata,
            )
            .await?)
    }
}

enum AttachmentRemovalId {
    Local(LocalAttachmentId),
    Cid(ContentId),
}

/// Utility type to wrap the queueing of attachment removal.
pub struct DraftAttachmentRemovalQueuer {
    id: MetadataId,
    attachment_id: AttachmentRemovalId,
}

impl DraftAttachmentRemovalQueuer {
    fn new(id: MetadataId, attachment_id: AttachmentRemovalId) -> Self {
        Self { id, attachment_id }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::attachment_remove",skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
    ) -> Result<QueuedActionOutput<AttachmentRemove>, MailContextError> {
        // Find existing attachment metadata.
        let attachment_metadata = match self.attachment_id {
            AttachmentRemovalId::Local(id) => {
                if let Some(attachment_metadata) =
                    DraftAttachmentMetadata::find_by_id(id, tether).await?
                {
                    attachment_metadata
                } else {
                    return Err(AttachmentUploadError::AttachmentMetadataNotFound(id).into());
                }
            }
            AttachmentRemovalId::Cid(id) => {
                if let Some(attachment_metadata) =
                    DraftAttachmentMetadata::find_with_content_id(self.id, id.clone(), tether)
                        .await?
                {
                    attachment_metadata
                } else {
                    return Err(AttachmentUploadError::AttachmentMetadataNotFoundCid(id).into());
                }
            }
        };

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail");
        // The removal action can only run when the current action completes.
        if let Some(action_id) = attachment_metadata.action_id {
            // Try to cancel the existing action if it hasn't run yet.
            if let Err(e) = queue.cancel(action_id).await {
                // Only fail if there is a real error
                match e {
                    QueuedError::ActionNotFound(_) | QueuedError::ActionInExecution(_) => {}
                    e => return Err(e.into()),
                }
            }
            metadata = metadata.with_sequential_dependency(action_id);
        };

        Ok(queue
            .queue_action_with_metadata(
                AttachmentRemove::new(self.id, attachment_metadata.local_attachment_id),
                metadata.build(),
            )
            .await?)
    }
}

/// Get the attachment staging path for a given draft with `metadata_id`.
pub fn draft_attachment_staging_path(
    context: &MailUserContext,
    metadata_id: MetadataId,
) -> PathBuf {
    context
        .attachment_staging_path()
        .join(metadata_id.to_string())
}

async fn queue_or_replace_draft_save(
    queue: &Queue,
    save_action: Save,
    metadata_id: MetadataId,
    last_draft_save_action_id: Option<ActionId>,
    other_dependencies: impl IntoIterator<Item = ActionId>,
) -> Result<QueuedActionOutput<Save>, ActionError<Save>> {
    let mut metadata_builder = MetadataBuilder::new()
        .with_resource(&metadata_id)
        .expect("This should never fail");
    if let Some(action_id) = last_draft_save_action_id {
        metadata_builder = metadata_builder.with_dependency(action_id);
    }
    let metadata = metadata_builder
        .with_dependencies(other_dependencies)
        .build();
    if let Some(previous_action_id) = last_draft_save_action_id {
        match queue
            .replace_or_queue_action_with_metadata(
                previous_action_id,
                save_action.clone(),
                metadata.clone(),
            )
            .await
        {
            Ok(v) => Ok(v),
            //TODO: More elegant solution
            // It is possible under certain circumstances to issue a replace
            // that can end of up in a cyclic dependency. E.g: Save(A) -> Upload Attachment (B) ->
            // Save (C). Replacing A with C will cause C to Depend on B and B on C rather
            // than A. Extra book keeping is required to prevent this. For now, in the interest
            // of saving time, we just queue the action normally when a cycle is detected.
            Err(ActionError::Queue(proton_action_queue::queue::Error::CyclicDependency)) => {
                queue
                    .queue_action_with_metadata(save_action, metadata)
                    .await
            }
            Err(e) => Err(e),
        }
    } else {
        queue
            .queue_action_with_metadata(save_action, metadata)
            .await
    }
}
