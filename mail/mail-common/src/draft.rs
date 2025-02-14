use std::sync::Arc;

use crate::actions::draft;
use crate::actions::draft::{Discard, Save, UndoSend};
use crate::datatypes::{Disposition, LocalAttachmentId, LocalMessageId, MimeType};
use crate::decrypted_message::DecryptedMessageBody;
use crate::draft::compose::{
    crate_draft_params, encrypt_draft_body, get_signature, patch_draft_with_reply_mode,
    prepare_html_reply, prepare_plain_text_reply, sanitize_draft_open, sanitize_draft_reply,
};
use crate::draft::recipients::{ContactGroupResolver, ProtonContactGroupResolver, RecipientList};
use crate::models::{
    DraftMetadata, DraftSendResult, DraftSendResultOrigin, EmbeddedAttachmentInfo, MailSettings,
    Message, MessageBodyMetadata, MetadataId,
};
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use derive_more::derive::TryFrom;
use futures::future::join3;
use proton_action_queue::action::{ActionId, MetadataBuilder};
use proton_action_queue::queue::{ActionError, ActionOutput, Queue, QueuedActionOutput};
use proton_api_core::consts::Mail;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::{AddressId, AuthId};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::prelude::DraftReplyOrForwardParams;
use proton_api_mail::services::proton::request_data::{DraftAction, DraftAttachmentKeyPackets};
use proton_api_mail::services::proton::response_data::Message as ApiMessage;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::attachment::{AttachmentDecryptionError, AttachmentEncryptionError};
use proton_crypto_inbox::keys::{PackageCryptoType, SessionKeyError};
use proton_crypto_inbox::message::MessageError;
use proton_mail_ids::LocalConversationId;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, SqliteError, ToSql, ToSqlOutput};
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether};
use tracing::{debug, error};

pub mod compose;
pub mod observers;
pub mod recipients;
pub(crate) mod send;

/// Potential draft specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error(transparent)]
    SaveOrSend(#[from] SaveOrSendError),
    #[error(transparent)]
    Discard(#[from] DiscardError),
    #[error(transparent)]
    Undo(#[from] UndoError),
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

/// Errors that occur during sending or saving a draft.
///
/// While these could in theory be separate errors, there is a lot of overlap
/// between the two, so we group them together. Additionally send always depends
/// on save, so these two will always come together.
#[derive(Debug, thiserror::Error)]
pub enum SaveOrSendError {
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
    #[error("Draft send failed: {0}")]
    SendMessage(#[from] PackageError),
    #[error("Draft has no recipients")]
    NoRecipients,
    #[error("Draft does not exist on server")]
    DraftDoesNotExistOnServer,
}

impl From<SaveOrSendError> for MailContextError {
    fn from(err: SaveOrSendError) -> Self {
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

/// Potential draft specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("Failed to encrypt package: {0}")]
    PackageBodyEncrypt(#[from] MessageError),
    #[error("Failed to load attachment content for mime body: {0}")]
    MimeBodyAttachmentLoad(#[from] ApiServiceError),
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
    #[error("Unknown error occurred while validating the recipient {0}")]
    UnknownRecipientValidationError(String),
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
    /// Records the last queued draft save id.
    pub last_draft_save_action_id: Option<ActionId>,
    #[debug(skip)]
    /// The decrypted message body.
    pub decrypted_body: DecryptedMessageBody,
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
    /// Open an existing draft with `message_id` and load all the relevant information.
    ///
    /// # Errors
    ///
    /// Returns error if the draft failed to load, the message can't be found
    /// or the message is not a draft.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(context))]
    pub async fn open(
        context: Arc<MailUserContext>,
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

        // First let's try to sync the body and metadata. If we can't we will fill it
        // ourselves.
        let (decrypted, sync_status) = if let Some(remote_id) = message.remote_id.clone() {
            match Message::force_sync_message_and_body(context.clone(), remote_id, true).await {
                Ok((message_new, decrypted)) => {
                    message = message_new;
                    (Some(decrypted), DraftSyncStatus::Synced)
                }
                // Handle network failure
                Err(MailContextError::Api(api_err)) if api_err.is_network_failure() => {
                    (None, DraftSyncStatus::Cached)
                }
                Err(e) => return Err(e),
            }
        } else {
            // If we have no remote id do not return cached status. As this implies the
            // draft was created locally and the save action has not yet executed.
            (None, DraftSyncStatus::Synced)
        };

        let mut decrypted = match decrypted {
            Some(d) => d,
            None => {
                debug!("Failed to sync draft from server, attempting to load from cache.");
                let Some(d) = Message::load_decrypted_message_from_cache(
                    Arc::clone(&context),
                    message.local_id.unwrap(),
                    tether,
                )
                .await
                .inspect_err(|e| error!("Failed to load decrypted data from cache: {e:?}"))?
                else {
                    return Err(OpenError::MessageBodyMissing(message.local_id.unwrap()).into());
                };
                d
            }
        };

        let metadata_id = if let Some(metadata) =
            DraftMetadata::find_by_message_id(message.local_id.unwrap(), tether)
                .await
                .inspect_err(|e| error!("Failed to load draft metadata: {e:?}"))?
        {
            debug!("Found existing metadata with id {}", metadata.id.unwrap());
            metadata.id.unwrap()
        } else {
            debug!("No metadata found, creating new entry");
            let mut metadata = DraftMetadata {
                id: None,
                local_message_id: Some(message.local_id.unwrap()),
                local_conversation_id: Some(message.local_conversation_id.unwrap()),
                local_parent_id: None,
                reply_mode: None,
                row_id: None,
            };
            let tx = tether.transaction().await?;
            metadata
                .save(&tx)
                .await
                .inspect_err(|e| error!("Failed to create new metadata: {e:?}"))?;
            tx.commit().await?;
            metadata.id.unwrap()
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

        // Transform body.
        sanitize_draft_open(context.session_id(), &mut decrypted);

        Ok((
            Self {
                metadata_id,
                sender: message.sender.address,
                to_list,
                cc_list,
                bcc_list,
                address_id: message.remote_address_id,
                subject: message.subject,
                send_result,
                last_draft_save_action_id: None,
                decrypted_body: decrypted,
            },
            sync_status,
        ))
    }

    /// Create a new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(stash))]
    pub async fn empty(stash: &Stash) -> Result<Self, MailContextError> {
        let mut tether = stash.connection();
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

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let address = &addresses[0];
        let tx = tether.transaction().await?;
        let metadata = DraftMetadata::empty(&tx)
            .await
            .inspect_err(|e| error!("Failed to create new empty draft metadata: {e:?}"))?;
        tx.commit().await?;

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
        let body = compose::get_signature(address, mail_settings);
        let decrypted_message_body =
            DecryptedMessageBody::new_draft(body, mail_settings.draft_mime_type);
        Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            decrypted_body: decrypted_message_body,
            send_result: None,
            last_draft_save_action_id: None,
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
            Message::load_decrypted_message_from_cache_without_attachment_preload(
                context, message_id, &tether,
            )
            .await
            .inspect_err(|e| error!("Failed to get source decrypted message: {e:?}"))?
        else {
            return Err(OpenError::MessageBodyMissing(message_id).into());
        };

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let tx = tether.transaction().await?;
        let metadata = DraftMetadata::reply(
            reply_mode,
            source_message.local_id.unwrap(),
            source_message.local_conversation_id.unwrap(),
            &tx,
        )
        .await
        .inspect_err(|e| error!("Failed to create new reply draft metadata: {e:?}"))?;
        tx.commit().await?;

        let contact_group_resolver = ProtonContactGroupResolver::new(&tether);

        Ok(Self::new_draft_reply(
            &contact_group_resolver,
            metadata.id.unwrap(),
            reply_mode,
            &address,
            &mail_settings,
            &source_message,
            source_message_body,
            use_utc,
            context.session_id(),
        )
        .await)
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
        session_id: &AuthId,
    ) -> Self {
        let mut body = get_signature(address, mail_settings);

        if mail_settings.draft_mime_type == MimeType::TextHtml {
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
        }

        let mut reply_draft_body =
            DecryptedMessageBody::new_draft(body, mail_settings.draft_mime_type);
        reply_draft_body.metadata.attachments = source_message_body.metadata.attachments;
        reply_draft_body.pgp_subject = source_message_body.pgp_subject;
        reply_draft_body.pgp_attachments = source_message_body.pgp_attachments;

        if reply_mode != ReplyMode::Forward {
            reply_draft_body
                .metadata
                .attachments
                .retain(|attachment| attachment.disposition == Disposition::Inline);
            if let Some(pgp_attachments) = &mut reply_draft_body.pgp_attachments {
                pgp_attachments.retain(|v| {
                    v.disposition
                        == proton_crypto_inbox::proton_crypto_inbox_mime::Disposition::Inline
                })
            }
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
            last_draft_save_action_id: None,
            decrypted_body: reply_draft_body,
        };

        patch_draft_with_reply_mode(
            contact_group_resolver,
            &mut draft,
            source_message,
            reply_mode,
            address,
        )
        .await;

        sanitize_draft_reply(session_id, &mut draft.decrypted_body);

        draft
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
        message: &Message,
        message_body_metadata: &MessageBodyMetadata,
        message_body: &str,
        draft_reply_or_forward_params: Option<DraftReplyOrForwardParams>,
    ) -> Result<ApiMessage, MailContextError> {
        let encrypted = encrypt_draft_body(context, &address_id, message_body).await?;
        let params = crate_draft_params(message, message_body_metadata, encrypted);

        let mut attachment_key_packets =
            DraftAttachmentKeyPackets::with_capacity(message_body_metadata.attachments.len());
        for attachment in &message_body_metadata.attachments {
            let Some(remote_id) = attachment.remote_id.clone() else {
                return Err(
                    AppError::AttachmentDoesNotHaveRemoteId(attachment.local_id.unwrap()).into(),
                );
            };
            let Some(key_packets) = attachment.key_packets.clone() else {
                return Err(SaveOrSendError::AttachmentDoesNotHaveKeyPackets(
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
        message: &Message,
        message_body_metadata: &MessageBodyMetadata,
        message_body: &str,
    ) -> Result<ApiMessage, MailContextError> {
        let encrypted = encrypt_draft_body(context, &address_id, message_body).await?;
        let params = crate_draft_params(message, message_body_metadata, encrypted);

        let mut attachment_key_packets =
            DraftAttachmentKeyPackets::with_capacity(message_body_metadata.attachments.len());
        for attachment in &message_body_metadata.attachments {
            let Some(remote_id) = attachment.remote_id.clone() else {
                return Err(
                    AppError::AttachmentDoesNotHaveRemoteId(attachment.local_id.unwrap()).into(),
                );
            };
            let Some(key_packets) = attachment.key_packets.clone() else {
                return Err(SaveOrSendError::AttachmentDoesNotHaveKeyPackets(
                    attachment.local_id.unwrap(),
                )
                .into());
            };
            attachment_key_packets.insert(remote_id, key_packets.value.clone());
        }

        match session
            .api()
            .update_draft(
                message.remote_id.clone().unwrap(),
                params,
                attachment_key_packets,
            )
            .await
        {
            Err(e) => {
                if let Some(proton_error) = e.to_proton_error() {
                    if proton_error.code == Mail::MessageAlreadySent as u32 {
                        return Err(SaveOrSendError::AlreadySent.into());
                    } else if proton_error.code == Mail::MessageUpdateDraftNotDraft as u32 {
                        return Err(
                            SaveOrSendError::MessageNotADraft(message.local_id.unwrap()).into()
                        );
                    } else if proton_error.code == Mail::MessageUpdateDraftNotExist as u32 {
                        return Err(SaveOrSendError::DraftDoesNotExistOnServer.into());
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
    ) -> Result<QueuedActionOutput<Save>, MailContextError> {
        let queued_output = self
            .to_save_action(self.last_draft_save_action_id)
            .queue(queue)
            .await?;
        self.last_draft_save_action_id = Some(queued_output.id);
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
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        let (save_output, send_output) = self
            .to_send_action(self.last_draft_save_action_id)?
            .queue(queue)
            .await?;
        self.last_draft_save_action_id = Some(save_output.id);
        Ok(send_output)
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

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    ///
    /// While we have our own instance variable for the `last_save_action_id`, it may
    /// be beneficial for users of this method to pass in an alternate source.
    ///
    pub fn to_save_action(&self, last_save_action_id: Option<ActionId>) -> DraftSaveActionQueuer {
        DraftSaveActionQueuer::new(
            self.metadata_id,
            Save::new(self, DraftSendResultOrigin::Save),
            last_save_action_id,
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
    pub fn to_send_action(
        &self,
        last_draft_save_action_id: Option<ActionId>,
    ) -> Result<DraftSendActionQueuer, Error> {
        if self.to_list.is_empty() && self.cc_list.is_empty() && self.bcc_list.is_empty() {
            return Err(SaveOrSendError::NoRecipients.into());
        }
        let save_action = Save::new(self, DraftSendResultOrigin::SaveBeforeSend);
        let send_action = draft::Send::new(self.metadata_id);
        let metadata_id = self.metadata_id;
        Ok(DraftSendActionQueuer::new(
            metadata_id,
            save_action,
            send_action,
            last_draft_save_action_id,
        ))
    }

    /// Create a discard action for the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    pub fn to_discard_action(&self) -> DraftDiscardActionQueuer {
        DraftDiscardActionQueuer::new(self.metadata_id, Discard::new(self))
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
    ) -> Result<ActionOutput<UndoSend>, ActionError<UndoSend>> {
        queue.apply_action(UndoSend::new(message_id)).await
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
        cid: &str,
    ) -> MailContextResult<EmbeddedAttachmentInfo> {
        self.decrypted_body.get_embedded_attachment(ctx, cid).await
    }
}

/// Utility type to disconnect queueing of the action from the [`Draft`] type in multithreaded
/// context.
pub struct DraftSaveActionQueuer {
    id: MetadataId,
    action: Save,
    previous_action_id: Option<ActionId>,
}

impl DraftSaveActionQueuer {
    fn new(id: MetadataId, action: Save, previous_action_id: Option<ActionId>) -> Self {
        Self {
            id,
            action,
            previous_action_id,
        }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::save",skip(self,queue))]
    pub async fn queue(self, queue: &Queue) -> Result<QueuedActionOutput<Save>, ActionError<Save>> {
        let metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail")
            .build();
        if let Some(previous_action_id) = self.previous_action_id {
            queue
                .replace_or_queue_action_with_metadata(previous_action_id, self.action, metadata)
                .await
        } else {
            queue
                .queue_action_with_metadata(self.action, metadata)
                .await
        }
    }
}

/// Utility type to disconnect queueing of the action from the [`Draft`] type in multithreaded
/// context.
pub struct DraftSendActionQueuer {
    id: MetadataId,
    save_action: Save,
    send_action: draft::Send,
    last_draft_save_action_id: Option<ActionId>,
}

impl DraftSendActionQueuer {
    fn new(
        id: MetadataId,
        save_action: Save,
        send_action: draft::Send,
        last_draft_save_action_id: Option<ActionId>,
    ) -> Self {
        Self {
            id,
            save_action,
            send_action,
            last_draft_save_action_id,
        }
    }

    /// Consume and queue this action.
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::send",skip(self,queue))]
    pub async fn queue(
        self,
        queue: &Queue,
    ) -> Result<(QueuedActionOutput<Save>, QueuedActionOutput<draft::Send>), MailContextError> {
        let save_metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail")
            .build();
        let save_output = if let Some(last_draft_save_action_id) = self.last_draft_save_action_id {
            queue
                .replace_or_queue_action_with_metadata(
                    last_draft_save_action_id,
                    self.save_action,
                    save_metadata,
                )
                .await?
        } else {
            queue
                .queue_action_with_metadata(self.save_action, save_metadata)
                .await?
        };
        let send_metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail")
            .with_dependency(save_output.id)
            .build();
        Ok((
            save_output,
            queue
                .queue_action_with_metadata(self.send_action, send_metadata)
                .await?,
        ))
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
    #[tracing::instrument(level=tracing::Level::DEBUG, name="draft::discard",skip(self,queue))]
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
