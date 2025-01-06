use std::sync::Arc;

use crate::actions::draft;
use crate::actions::draft::Save;
use crate::cache::CacheMessageKey;
use crate::datatypes::{Disposition, MimeType};
use crate::decrypted_message::StorableMessageBody;
use crate::draft::compose::{
    crate_draft_params, encrypt_draft_body, get_signature, patch_draft_with_reply_mode,
    prepare_html_reply, prepare_plain_text_reply,
};
use crate::draft::recipients::{ContactGroupResolver, RecipientList};
use crate::models::{
    Attachment, DraftMetadata, MailSettings, Message, MessageBodyMetadata, MetadataId,
};
use crate::{AppError, MailContextError, MailUserContext};
use futures::future::join3;
use proton_action_queue::action::Metadata;
use proton_action_queue::queue::Queue;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::AddressId;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::request_data::{DraftAction, DraftAttachmentKeyPackets};
use proton_api_mail::services::proton::response_data::Message as ApiMessage;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use proton_crypto_inbox::attachment::{AttachmentDecryptionError, AttachmentEncryptionError};
use proton_crypto_inbox::keys::{PackageCryptoType, SessionKeyError};
use proton_crypto_inbox::message::MessageError;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, SqliteError, ToSql, ToSqlOutput};
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, StashError, Tether};
use tracing::{debug, error, warn};

pub mod compose;
pub mod recipients;
pub(crate) mod send;

/// Potential draft specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No addresses found for current user")]
    UserHasNoAddresses,
    #[error("User Address {0} not found")]
    AddressNotFound(AddressId),
    #[error("User Address {0} has no primary key")]
    AddressWithoutPrimaryKey(AddressId),
    #[error("Message {0} is not a draft")]
    MessageNotADraft(LocalId),
    #[error("Create Metadata not found for {0}")]
    CreateMetadataNotFound(LocalId),
    #[error("Message Body for {0} missing")]
    MessageBodyMissing(LocalId),
    #[error("Attachment {0} does not have key packets")]
    AttachmentDoesNotHaveKeyPackets(LocalId),
    #[error("Can't reply or forward to a draft message {0}")]
    ReplyOrForwardToDraft(LocalId),
    #[error("Metadata with Id {0} does not exist")]
    MetadataNotFound(MetadataId),
    #[error("Draft has no message")]
    DraftWithoutMessage,
    #[error("Draft send failed: {0}")]
    SendMessage(#[from] PackageError),
    #[error("Draft has no recipients")]
    NoRecipients,
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
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
        match value.as_i64()? {
            0 => Ok(ReplyMode::Sender),
            1 => Ok(ReplyMode::All),
            2 => Ok(ReplyMode::Forward),
            v => Err(FromSqlError::OutOfRange(v)),
        }
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
#[derive(Debug)]
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
    /// Unencrypted body of the draft.
    pub body: String,
    /// Attachment associated with this draft
    pub attachments: Vec<Attachment>,
    /// Draft's mime type
    pub mime_type: MimeType,
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
        message_id: LocalId,
    ) -> Result<Self, MailContextError> {
        let tether = &mut context.user_stash().connection();

        let Some(mut message) = Message::find_by_id(message_id, tether).await? else {
            error!("Opened message as draft that does not exist.");
            return Err(AppError::MessageMissing(message_id).into());
        };

        if !message.flags.is_draft() {
            error!("Opened a non-draft message as a draft");
            return Err(Error::MessageNotADraft(message_id).into());
        }

        // First let's try to sync the body and metadata. If we can't we will fill it
        // ourselves.
        let (body, body_metadata) = if let Some(remote_id) = message.remote_id.clone() {
            match Message::force_sync_message_and_body(context.clone(), remote_id).await {
                Ok((message_new, body_metadata, body)) => {
                    message = message_new;
                    (Some(body), Some(body_metadata))
                }
                // Handle network failure
                Err(MailContextError::Api(api_err)) if api_err.is_network_failure() => {
                    warn!("Api error syncing message");
                    (None, None)
                }
                Err(e) => return Err(e),
            }
        } else {
            (None, None)
        };

        // Load body metadata if not re-synced.
        let body_metadata = if let Some(body_metadata) = body_metadata {
            body_metadata
        } else {
            debug!("Message body metadata not present. Querying the db...");
            let Some(body_metadata) =
                MessageBodyMetadata::for_message(message.local_id.unwrap(), tether).await?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };

            body_metadata
        };

        // Load body from cache if not resynced.
        let body = if let Some(body) = body {
            body
        } else {
            debug!("Message body not present. Looking in the cache...");
            let key = CacheMessageKey::from(&message);
            let Some(message_body_reader) = context.messages_cache().get_item(&key)? else {
                return Err(AppError::MessageBodyMissing(message.local_id.unwrap()).into());
            };

            let body = StorableMessageBody::from_reader(message_body_reader)
                .inspect_err(|e| error!("Failed to load message body: {e}"))?;

            body.body
        };

        let metadata_id = if let Some(metadata) =
            DraftMetadata::find_by_message_id(message.local_id.unwrap(), tether)
                .await
                .inspect_err(|e| error!("Failed to load draft metadata: {e}"))?
        {
            debug!("Found existing metadata with id {}", metadata.id.unwrap());
            metadata.id.unwrap()
        } else {
            debug!("No metadata found, creating new entry");
            let mut metadata = DraftMetadata {
                id: None,
                local_message_id: Some(message.local_id.unwrap()),
                local_conversation_id: Some(message.local_id.unwrap()),
                local_parent_id: None,
                reply_mode: None,
                row_id: None,
            };
            let tx = tether.transaction().await?;
            metadata
                .save(&tx)
                .await
                .inspect_err(|e| error!("Failed to create new metadata: {e}"))?;
            tx.commit().await?;
            metadata.id.unwrap()
        };

        let context = &*context;
        let (to_list, cc_list, bcc_list) = join3(
            RecipientList::from_message_recipients(context, message.to_list.value),
            RecipientList::from_message_recipients(context, message.cc_list.value),
            RecipientList::from_message_recipients(context, message.bcc_list.value),
        )
        .await;
        Ok(Self {
            metadata_id,
            sender: message.sender.address,
            to_list,
            cc_list,
            bcc_list,
            address_id: message.remote_address_id,
            subject: message.subject,
            body,
            attachments: body_metadata.attachments,
            mime_type: body_metadata.mime_type,
        })
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
                error!("Failed to load addresses: {e}");
            })?;

        if addresses.is_empty() {
            error!("No addresses found for current user");
            return Err(Error::UserHasNoAddresses.into());
        }

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let address = &addresses[0];
        let tx = tether.transaction().await?;
        let metadata = DraftMetadata::empty(&tx)
            .await
            .inspect_err(|e| error!("Failed to create new empty draft metadata: {e}"))?;
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
        Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            body,
            attachments: Vec::new(),
            mime_type: mail_settings.draft_mime_type,
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
        message_id: LocalId,
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
            return Err(Error::ReplyOrForwardToDraft(message_id).into());
        }

        // Source message much have a remote id.
        if source_message.remote_id.is_none() {
            return Err(AppError::MessageHasNoRemoteId(message_id).into());
        }

        // Message body must be present to create a reply.
        let Some(source_body_metadata) = MessageBodyMetadata::find_first(
            "WHERE local_message_id=?",
            params![message_id],
            &tether,
        )
        .await
        .inspect_err(|e| {
            error!("Failed to load source message body: {e}");
        })?
        else {
            error!("Source message body is not present");
            return Err(Error::MessageBodyMissing(message_id).into());
        };

        // Find out which address this message has and use that to craft te reply.
        let Some(address) =
            Address::find_by_id(source_message.remote_address_id.clone(), &tether).await?
        else {
            return Err(Error::AddressNotFound(source_message.remote_address_id.clone()).into());
        };

        let key = CacheMessageKey::from(&source_message);
        let Some(source_body_reader) =
            context.messages_cache().get_item(&key).inspect_err(|e| {
                error!("Failed to get source body: {e}");
            })?
        else {
            error!("Could not load message body");
            return Err(Error::MessageBodyMissing(message_id).into());
        };

        let source_body = StorableMessageBody::from_reader(source_body_reader)
            .inspect_err(|e| {
                error!("Failed to read body into string: {e}");
            })?
            .body;

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let tx = tether.transaction().await?;
        let metadata = DraftMetadata::reply(
            reply_mode,
            source_message.local_id.unwrap(),
            source_message.local_conversation_id.unwrap(),
            &tx,
        )
        .await
        .inspect_err(|e| error!("Failed to create new reply draft metadata: {e}"))?;
        tx.commit().await?;

        Ok(Self::new_draft_reply(
            context,
            metadata.id.unwrap(),
            reply_mode,
            &address,
            &mail_settings,
            &source_message,
            source_body_metadata,
            source_body,
            use_utc,
        )
        .await)
    }

    /// Create a draft reply.
    ///
    /// # Params
    ///
    /// `metadata_id`    - Metadata id for this draft.
    /// `reply_mode`     - Draft reply mode.
    /// `address`        - Sender address.
    /// `source_message` - Metadata of the message we are replying to.
    /// `source_body_metadata` - Body metadata of the message we are replying to.
    /// `source_body`          - Body of the message we are replying to.
    /// `use_utc`              - Whether to use utc over local timezone.
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
        source_body_metadata: MessageBodyMetadata,
        source_body: String,
        use_utc: bool,
    ) -> Self {
        let mut body = get_signature(address, mail_settings);

        if mail_settings.draft_mime_type == MimeType::TextHtml {
            prepare_html_reply(&mut body, source_message, &source_body, use_utc);
        } else {
            prepare_plain_text_reply(
                &mut body,
                source_message,
                source_body,
                source_body_metadata.mime_type,
                use_utc,
            );
        }

        let mut draft = Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            body,
            attachments: if reply_mode == ReplyMode::Forward {
                source_body_metadata.attachments
            } else {
                source_body_metadata
                    .attachments
                    .into_iter()
                    .filter(|attachment| attachment.disposition == Disposition::Inline)
                    .collect()
            },
            mime_type: mail_settings.draft_mime_type,
        };

        patch_draft_with_reply_mode(
            contact_group_resolver,
            &mut draft,
            source_message,
            reply_mode,
        )
        .await;

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
        action: DraftAction,
        message: &Message,
        message_body_metadata: &MessageBodyMetadata,
        message_body: &str,
        parent_id: Option<RemoteId>,
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
                return Err(
                    Error::AttachmentDoesNotHaveKeyPackets(attachment.local_id.unwrap()).into(),
                );
            };
            attachment_key_packets.insert(remote_id, key_packets.value.clone());
        }

        let response = session
            .api()
            .create_draft(
                params,
                action,
                attachment_key_packets,
                parent_id.map(Into::into),
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
                return Err(
                    Error::AttachmentDoesNotHaveKeyPackets(attachment.local_id.unwrap()).into(),
                );
            };
            attachment_key_packets.insert(remote_id, key_packets.value.clone());
        }

        let response = session
            .api()
            .update_draft(
                message.remote_id.clone().unwrap(),
                params,
                attachment_key_packets,
            )
            .await?;
        Ok(response.message)
    }

    /// Apply an action which will create a new draft.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(queue))]
    pub async fn save(&self, queue: &Queue) -> Result<(), MailContextError> {
        queue.queue_action(self.to_save_action()).await?;
        Ok(())
    }

    /// Apply an action which will send this draft.
    ///
    /// This requires both a save and send action that need to be chained.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(queue, save_action,send_action))]
    pub async fn send(
        queue: &Queue,
        save_action: Save,
        send_action: draft::Send,
    ) -> Result<(), MailContextError> {
        //TODO: Atomic commit - needs queue modifications
        let save_output = queue.queue_action(save_action).await?;
        queue
            .queue_action_with_metadata(
                send_action,
                Metadata::builder().with_dependency(save_output.id).build(),
            )
            .await?;
        Ok(())
    }

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub fn to_save_action(&self) -> Save {
        Save::new(self)
    }

    /// Create a save action for the current state of the draft.
    ///
    /// This method is here to provide greater flexibility of integration
    /// when used in multithreaded contexts.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub fn to_send_action(&self) -> Result<draft::Send, Error> {
        if self.to_list.is_empty() && self.cc_list.is_empty() && self.bcc_list.is_empty() {
            return Err(Error::NoRecipients);
        }

        Ok(draft::Send::new(self.metadata_id))
    }

    /// Get the message id associated with this draft.
    ///
    /// This method can return `None` if the message has not been
    /// created yet.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn message_id(&self, tether: &Tether) -> Result<Option<LocalId>, StashError> {
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
    pub async fn conversation_id(&self, tether: &Tether) -> Result<Option<LocalId>, StashError> {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, tether).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_conversation_id)
    }
}
