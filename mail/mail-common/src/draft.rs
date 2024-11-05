use crate::actions::draft::Save;
use crate::cache::CacheMessageKey;
use crate::datatypes::{Disposition, MessageAddress, MimeType, PmSignature};
use crate::decrypted_message::StorableMessageBody;
use crate::models::{
    Attachment, DraftMetadata, MailSettings, Message, MessageBodyMetadata, MetadataId,
};
use crate::{AppError, MailContextError, MailUserContext};
use chrono::DateTime;
use proton_action_queue::queue::Queue;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::response_data::Message as ApiMessage;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use proton_core_common::KeyHandlingError;
use proton_crypto_inbox::message::{EncryptableDraft, EncryptedDraft};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, SqliteError, ToSql, ToSqlOutput};
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, StashError};
use std::fmt::Display;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

#[cfg(test)]
#[path = "tests/draft.rs"]
mod tests;

/// Potential draft specific errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No addresses found for current user")]
    UserHasNoAddresses,
    #[error("User Address {0} not found")]
    AddressNotFound(RemoteId),
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
#[derive(Debug, Clone)]
pub struct Draft {
    /// Id of the associated metadata.
    pub metadata_id: MetadataId,
    /// Sender email address
    pub sender: String,
    /// To Recipients addresses
    pub to_list: Vec<String>,
    /// CC Recipients addresses
    pub cc_list: Vec<String>,
    /// BCC recipients addresses
    pub bcc_list: Vec<String>,
    /// Address used to send the message
    pub address_id: RemoteId,
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
        context: &MailUserContext,
        message_id: LocalId,
    ) -> Result<Self, MailContextError> {
        let Some(message) = Message::find_by_id(message_id, context.user_stash()).await? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        if !message.flags.is_draft() {
            return Err(Error::MessageNotADraft(message_id).into());
        }

        let body = Message::message_body(context, message_id)
            .await
            .inspect_err(|e| {
                error!("Failed to get message body from cache: {e}");
            })?;

        let tether = context.user_stash().connection();

        let metadata_id = if let Some(metadata) =
            DraftMetadata::find_by_message_id(message.local_id.unwrap(), &tether)
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
                stash: None,
            };
            metadata
                .save_using(&tether)
                .await
                .inspect_err(|e| error!("Failed to create new metadata: {e}"))?;
            metadata.id.unwrap()
        };

        Ok(Self {
            metadata_id,
            sender: message.sender.address,
            to_list: message
                .to_list
                .value
                .into_iter()
                .map(|v| v.address)
                .collect(),
            cc_list: message
                .cc_list
                .value
                .into_iter()
                .map(|v| v.address)
                .collect(),
            bcc_list: message
                .bcc_list
                .value
                .into_iter()
                .map(|v| v.address)
                .collect(),
            address_id: message.remote_address_id,
            subject: message.subject,
            body: body.body,
            attachments: body.metadata.attachments,
            mime_type: body.metadata.mime_type,
        })
    }

    /// Create a new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(interface))]
    pub async fn empty<A>(interface: &A) -> Result<Self, MailContextError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        // Default address should have display_order 0
        let addresses = Address::find(
            "ORDER BY display_order ASC LIMIT 1",
            vec![],
            interface,
            None,
        )
        .await
        .inspect_err(|e| {
            error!("Failed to load addresses: {e}");
        })?;

        if addresses.is_empty() {
            error!("No addresses found for current user");
            return Err(Error::UserHasNoAddresses.into());
        }

        let mail_settings = MailSettings::get(interface).await?.unwrap_or_default();
        let address = &addresses[0];

        let metadata = DraftMetadata::empty(interface)
            .await
            .inspect_err(|e| error!("Failed to create new empty draft metadata: {e}"))?;

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
        let body = get_signature(address, mail_settings);
        Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: Vec::new(),
            cc_list: Vec::new(),
            bcc_list: Vec::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: DEFAULT_SUBJECT.to_owned(),
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
        let tether = context.user_stash().connection();
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

        let key = CacheMessageKey::from_message(&source_message, &tether);
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

        let metadata = DraftMetadata::reply(
            reply_mode,
            source_message.local_id.unwrap(),
            source_message.local_conversation_id.unwrap(),
            &tether,
        )
        .await
        .inspect_err(|e| error!("Failed to create new reply draft metadata: {e}"))?;

        Ok(Self::new_draft_reply(
            metadata.id.unwrap(),
            reply_mode,
            &address,
            &mail_settings,
            &source_message,
            source_body_metadata,
            source_body,
            use_utc,
        ))
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
    fn new_draft_reply(
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
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
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

        patch_draft_with_reply_mode(&mut draft, source_message, reply_mode);

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
        address_id: RemoteId,
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
            attachment_key_packets.insert(remote_id.into(), key_packets.value.clone());
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
        address_id: RemoteId,
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
            attachment_key_packets.insert(remote_id.into(), key_packets.value.clone());
        }

        let response = session
            .api()
            .update_draft(
                message.remote_id.clone().unwrap().into(),
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

    /// Get the message id associated with this draft.
    ///
    /// This method can return `None` if the message has not been
    /// created yet.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn message_id<A>(&self, interface: &A) -> Result<Option<LocalId>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, interface).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_message_id)
    }

    /// Get the conversation id associated with this draft.
    ///
    /// This method can return `None` if the draft is a new empty reply
    /// and the conversation has not yet been created.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn conversation_id<A>(&self, interface: &A) -> Result<Option<LocalId>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, interface).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_conversation_id)
    }
}

/// Copy all the data from the `source_message` into `message` taking
/// into account `reply_mode` of the draft.
fn patch_draft_with_reply_mode(draft: &mut Draft, source_message: &Message, reply_mode: ReplyMode) {
    // Copy over the addresses based on reply mode
    match reply_mode {
        ReplyMode::Sender => {
            draft.to_list = vec![source_message.sender.address.clone()];
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::All => {
            draft.to_list = vec![source_message.sender.address.clone()];
            draft.to_list.extend(
                source_message
                    .to_list
                    .value
                    .iter()
                    .map(|v| v.address.clone()),
            );
            draft.cc_list = source_message
                .cc_list
                .value
                .iter()
                .map(|v| v.address.clone())
                .collect();
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::Forward => {
            draft.subject = apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject);
        }
    }
}

/// Create draft params from `message` and `message_body_metadata`
fn crate_draft_params(
    message: &Message,
    message_body_metadata: &MessageBodyMetadata,
    encrypted_draft: EncryptedDraft,
) -> DraftParams {
    DraftParams {
        subject: message.subject.clone(),
        unread: message.unread,
        sender: DraftSender {
            address: message.sender.address.clone(),
            name: message.sender.name.clone(),
        },
        to_list: recipient_from_message_sender(&message.to_list.value),
        cc_list: recipient_from_message_sender(&message.cc_list.value),
        bcc_list: recipient_from_message_sender(&message.bcc_list.value),
        external_id: message.external_id.clone().map(|id| id.to_string()),
        draft_flags: 0,
        body: encrypted_draft,
        mime_type: message_body_metadata.mime_type.into(),
    }
}

/// Build signature from mail settings.
fn get_signature(address: &Address, mail_settings: &MailSettings) -> String {
    let line_break = if mail_settings.draft_mime_type == MimeType::TextHtml {
        HTML_LINE_BREAK
    } else {
        "\n"
    };
    let mut signature = if mail_settings.signature.is_empty() {
        address.signature.clone()
    } else if address.signature.is_empty() {
        mail_settings.signature.clone()
    } else {
        format!(
            "{}{line_break}{line_break}{}",
            address.signature, mail_settings.signature
        )
    };

    if mail_settings.pm_signature != PmSignature::Disabled {
        signature.push_str(line_break);
        signature.push_str(line_break);
        if mail_settings.draft_mime_type == MimeType::TextHtml {
            signature.push_str(PM_SIGNATURE_HTML);
        } else {
            signature.push_str(PM_SIGNATURE_PLAIN_TEXT);
        }
    }

    if !signature.is_empty() {
        signature.insert_str(0, &format!("{line_break}{line_break}"));
    }

    signature
}

fn recipient_from_message_sender(recipients: &[MessageAddress]) -> Vec<DraftRecipient> {
    recipients
        .iter()
        .map(|v| {
            DraftRecipient {
                address: v.address.clone(),
                name: v.name.clone(),
                // TODO: where to get group from?
                group: None,
            }
        })
        .collect()
}

struct DraftBody<'b> {
    body: &'b str,
}

impl EncryptableDraft for DraftBody<'_> {
    fn plaintext_message_body(&self) -> &[u8] {
        self.body.as_bytes()
    }
}

/// Encrypt the `body` with the key for `address_id`.
async fn encrypt_draft_body(
    ctx: &MailUserContext,
    address_id: &RemoteId,
    body: &str,
) -> Result<EncryptedDraft, MailContextError> {
    let draft_body = DraftBody { body };
    let pgp_provider = new_pgp_provider();
    let unlocked_keys = ctx.unlocked_address_keys(&pgp_provider, address_id).await?;
    let Some(draft_encryption_key) = unlocked_keys.primary() else {
        error!(
            "Unable to find the primary address key to encrypt the draft for address with id: {address_id}"
        );
        return Err(MailContextError::PGPKeyAccess(
            KeyHandlingError::NoPrimaryKey,
        ));
    };
    draft_body
        .encrypt_draft_body(&pgp_provider, draft_encryption_key)
        .map_err(|e| {
            error!("Failed to encrypt draft: {e}");
            MailContextError::Crypto
        })
}

/// Create a new timestamp.
pub(crate) fn create_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs()
}

/// Generate HTML reply body for a message.
fn prepare_html_reply(output: &mut String, message: &Message, original_body: &str, use_utc: bool) {
    let sender_reply = generate_sender_reply(
        &message.sender,
        format_date_from_timestamp(message.time, use_utc),
    );
    output.reserve((ORIGINAL_MESSAGE_BLOCK.len() * 2) + original_body.len());
    output.push_str(BEGIN_QUOTE);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(ORIGINAL_MESSAGE_BLOCK);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(&sender_reply);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(BEGIN_BLOCKQUOTE);
    output.push_str(original_body);
    output.push_str(CLOSE_BLOCKQUOTE);
    output.push_str(CLOSE_QUOTE);
}

/// Generate a plain text reply body for a message.
fn prepare_plain_text_reply(
    output: &mut String,
    message: &Message,
    mut original_body: String,
    original_body_mime_type: MimeType,
    use_utc: bool,
) {
    // Convert body to text if source is html
    if original_body_mime_type == MimeType::TextHtml {
        original_body = html_to_text(original_body);
    }

    let sender_reply = generate_sender_reply(
        &message.sender,
        format_date_from_timestamp(message.time, use_utc),
    );

    output.reserve((ORIGINAL_MESSAGE_BLOCK.len() * 2) + original_body.len());
    output.push('\n');
    output.push('\n');
    output.push_str(ORIGINAL_MESSAGE_BLOCK);
    output.push('\n');
    output.push_str(&sender_reply);
    output.push('\n');
    output.push_str(&original_body);
}

/// Converts htm to plain text. If an error occurs the original messages
/// is rerturned.
fn html_to_text(input: String) -> String {
    let cursor = io::Cursor::new(&input);
    let config = html2text::config::plain();
    match config.string_from_read(cursor, 80) {
        Ok(text_body) => text_body,
        Err(e) => {
            error!("Failed to convert html to text: {e}");
            input
        }
    }
}

/// Generates a reply similar to:
/// > On Tuesday, 01/01/2024 14:25, Slack <notification@slack.com> wrote:
fn generate_sender_reply(sender: &MessageAddress, formatted_date: String) -> String {
    if !sender.name.is_empty() && !sender.address.is_empty() {
        format!(
            "{formatted_date} {} <{}> wrote:",
            sender.name, sender.address
        )
    } else if !sender.name.is_empty() {
        format!("{formatted_date} {} wrote:", sender.name)
    } else {
        format!("{formatted_date} {} wrote:", sender.address)
    }
}

fn format_date_from_timestamp(timestamp: u64, use_utc: bool) -> String {
    if use_utc {
        format_date(date_from_timestamp::<chrono::Utc>(timestamp))
    } else {
        format_date(date_from_timestamp::<chrono::Local>(timestamp))
    }
}

fn date_from_timestamp<Tz: chrono::TimeZone>(timestamp: u64) -> DateTime<Tz>
where
    DateTime<Tz>: From<DateTime<chrono::Utc>>,
{
    let timestamp_i64 = i64::try_from(timestamp).unwrap_or(0);
    DateTime::<chrono::Utc>::from_timestamp(timestamp_i64, 0)
        .unwrap_or_default()
        .into()
}

fn format_date<Tz: chrono::TimeZone>(date: DateTime<Tz>) -> String
where
    <Tz as chrono::TimeZone>::Offset: Display,
{
    //On Tuesday, 01/01/2024 14:25
    // Localize date representation
    date.format("On %A, %x at %H:%M").to_string()
}

pub const REPLY_PREFIX: &str = "Re: ";
pub const FORWARD_PREFIX: &str = "Fwd: ";

pub const DEFAULT_SUBJECT: &str = "(No Subject)";
pub const ORIGINAL_MESSAGE_BLOCK: &str = "-------- Original Message --------";
pub const BEGIN_QUOTE: &str = "<div class=\"protonmail_quote\">";
pub const BEGIN_BLOCKQUOTE: &str = "<blockquote class=\"protonmail_quote\">";
pub const CLOSE_QUOTE: &str = "</div>";
pub const CLOSE_BLOCKQUOTE: &str = "</blockquote>";
pub const HTML_LINE_BREAK: &str = "<br>";

const PM_SIGNATURE_HTML: &str = r#"Sent with <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> secure email."#;

const PM_SIGNATURE_PLAIN_TEXT: &str = "Sent with Proton Mail secure email.";

fn apply_prefix_to_subject(prefix: &str, subject: &str) -> String {
    let trimmed_subject = subject.trim();
    if trimmed_subject.starts_with(prefix) {
        trimmed_subject.to_string()
    } else {
        format!("{prefix} {trimmed_subject}")
    }
}
