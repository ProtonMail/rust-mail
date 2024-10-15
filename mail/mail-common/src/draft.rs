use crate::actions::draft::Create;
use crate::cache::{CacheMessageConfig, CacheMessageKey};
use crate::datatypes::{MessageAddress, MessageAddresses, MessageFlags, SystemLabelId};
use crate::models::{
    Conversation, Label, MailSettings, Message, MessageBodyMetadata, NewDraftMetadata,
};
use crate::{AppError, MailContextError, MailUserContext};
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::response_data::Message as ApiMessage;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::cache::ProtonCache;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use proton_crypto_inbox::message::{EncryptableDraft, EncryptedDraft};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_sqlite3::rusqlite;
use rusqlite::types::{FromSqlError, FromSqlResult, ValueRef};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, ToSql, ToSqlOutput};
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

#[cfg(test)]
#[path = "tests/draft.rs"]
mod tests;

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
    #[error("Can't reply or forward to a draft message {0}")]
    ReplyOrForwardToDraft(LocalId),
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
#[derive(Debug)]
pub struct Draft {
    /// Sender
    pub sender: MessageAddress,
    /// To Recipients
    pub to_list: Vec<MessageAddress>,
    /// CC Recipients
    pub cc_list: Vec<MessageAddress>,
    /// BCC recipients
    pub bcc_list: Vec<MessageAddress>,
    /// Local id of the message this conversation belongs to
    pub message_id: LocalId,
    /// Local id of the conversation this message belongs to
    pub conversation_id: LocalId,
    /// Address used to send the message
    pub address_id: RemoteId,
    /// Draft subject
    pub subject: String,
    /// Unencrypted body of the draft.
    pub body: String,
}

impl Draft {
    /// Open an existing draft with `message_id` and load all the relevant information.
    ///
    /// # Errors
    ///
    /// Returns error if the draft failed to load, the message can't be found
    /// or the message is not a draft.
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

        //TODO: Handle attachments (ET-1362)
        Ok(Self {
            sender: message.sender,
            to_list: message.to_list.value,
            cc_list: message.cc_list.value,
            bcc_list: message.bcc_list.value,
            message_id,
            conversation_id: message.local_conversation_id.unwrap(),
            address_id: message.remote_address_id,
            subject: message.subject,
            body: body.body,
        })
    }

    /// Create a new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(context,interface))]
    pub(crate) async fn empty<A>(
        context: &MailUserContext,
        interface: &A,
    ) -> Result<Self, MailContextError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_draft_label_id = local_draft_label_id(interface).await?;

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
        let conv_display_order = Conversation::next_display_order(interface)
            .await
            .inspect_err(|e| {
                error!("Failed to get conversation display order: {e}");
            })?;
        let msg_display_order = Message::next_display_order(interface)
            .await
            .inspect_err(|e| {
                error!("Failed to get message display order: {e}");
            })?;

        let address = &addresses[0];

        let body = get_signature(address, &mail_settings);

        let mut message = create_new_message(address, msg_display_order, body.len());

        // Create new conversation
        let mut conversation = Conversation {
            num_messages: 1,
            senders: MessageAddresses {
                value: vec![message.sender.clone()],
            },
            subject: message.subject.clone(),
            size: message.size,
            is_known: true,
            has_messages: true,
            display_order: conv_display_order,
            ..Default::default()
        };

        conversation
            .save_using(interface)
            .await
            .inspect_err(|e| error!("Failed to save conversation :{e}"))?;

        message.local_conversation_id = conversation.local_id;

        message.save_using(interface).await.inspect_err(|e| {
            error!("Error creating new draft locally: {e}");
        })?;

        //NOTE: Headers are initialized by the server.
        let mut message_body_metadata = MessageBodyMetadata {
            local_message_id: message.local_id,
            remote_message_id: None,
            header: "".to_string(),
            mime_type: mail_settings.draft_mime_type,
            parsed_headers: Default::default(),
            row_id: None,
            stash: None,
        };

        // Apply drafts label
        Conversation::apply_label(
            local_draft_label_id,
            std::iter::once(conversation.local_id.unwrap()),
            interface,
        )
        .await
        .inspect_err(|e| error!("Failed to apply Draft label: {e}"))?;

        message_body_metadata
            .save_using(interface)
            .await
            .inspect_err(|e| {
                error!("Failed to save new draft body metadata :{e}");
            })?;

        let mut metadata = NewDraftMetadata {
            local_message_id: message.local_id.unwrap(),
            remote_parent_id: None,
            reply_mode: None,
            row_id: None,
            stash: None,
        };

        metadata.save_using(interface).await.inspect_err(|e| {
            error!("Failed to save new draft metadata :{e}");
        })?;

        // Store body in cache.
        store_body_in_cache(context.messages_cache(), &message, &body, interface).inspect_err(
            |e| {
                error!("Failed to store draft body in cache :{e}");
            },
        )?;

        Ok(Self {
            sender: message.sender,
            to_list: message.to_list.value,
            cc_list: message.cc_list.value,
            bcc_list: message.bcc_list.value,
            message_id: message.local_id.unwrap(),
            conversation_id: message.local_conversation_id.unwrap(),
            address_id: address.remote_id.clone().unwrap(),
            subject: message.subject,
            body,
        })
    }

    /// Create a draft as reply/forward to an existing message with `message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if we can not load or modify the required data or write the
    /// body into the cache.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(context,interface))]
    pub(crate) async fn reply<A>(
        context: &MailUserContext,
        message_id: LocalId,
        reply_mode: ReplyMode,
        interface: &A,
    ) -> Result<Self, MailContextError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let local_draft_label_id = local_draft_label_id(interface).await?;

        // Load the message we reply to.
        let Some(mut source_message) = Message::find_by_id(message_id, interface).await? else {
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
        //TODO: Handle attachments (ET-1362)
        let Some(_) = MessageBodyMetadata::find_first(
            "WHERE local_message_id=?",
            params![message_id],
            interface,
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
            Address::find_by_id(source_message.remote_address_id.clone(), interface).await?
        else {
            return Err(Error::AddressNotFound(source_message.remote_address_id.clone()).into());
        };

        let mail_settings = MailSettings::get(interface).await?.unwrap_or_default();

        let display_order = Message::next_display_order(interface)
            .await
            .inspect_err(|e| {
                error!("Failed to get message display order: {e}");
            })?;

        //TODO: Patch body for reply (ET-1361)
        let body = get_signature(&address, &mail_settings);

        let mut message = create_new_draft_with_reply_mode(
            &mut source_message,
            reply_mode,
            &address,
            display_order,
            body.len(),
        );

        source_message
            .save_using(interface)
            .await
            .inspect_err(|e| {
                error!("Failed to update source message: {e}");
            })?;

        message.save_using(interface).await.inspect_err(|e| {
            error!("Error creating new draft locally: {e}");
        })?;

        //TODO: Handle attachments (ET-1362)
        //      - Reply / Reply All: Transfer embedded images
        //      - Forward, transfer all attachments
        let mut message_body_metadata = MessageBodyMetadata {
            local_message_id: message.local_id,
            remote_message_id: None,
            header: "".to_string(),
            mime_type: mail_settings.draft_mime_type,
            parsed_headers: Default::default(),
            row_id: None,
            stash: None,
        };

        message_body_metadata
            .save_using(interface)
            .await
            .map_err(|e| {
                error!("Failed to save new draft body metadata :{e}");
                e
            })?;

        // Apply draft an all other message labels
        Message::apply_label(
            local_draft_label_id,
            std::iter::once(message.local_id.unwrap()),
            interface,
        )
        .await
        .inspect_err(|e| error!("Failed to apply draft label: {e}"))?;

        let mut metadata = NewDraftMetadata {
            local_message_id: message.local_id.unwrap(),
            remote_parent_id: Some(source_message.remote_id.unwrap()),
            reply_mode: Some(reply_mode),
            row_id: None,
            stash: None,
        };

        metadata.save_using(interface).await.inspect_err(|e| {
            error!("Failed to save new draft metadata :{e}");
        })?;

        // Store body in cache.
        store_body_in_cache(context.messages_cache(), &message, &body, interface).inspect_err(
            |e| {
                error!("Failed to store draft body in cache :{e}");
            },
        )?;

        Ok(Self {
            sender: message.sender,
            to_list: message.to_list.value,
            cc_list: message.cc_list.value,
            bcc_list: message.bcc_list.value,
            message_id: message.local_id.unwrap(),
            conversation_id: message.local_conversation_id.unwrap(),
            address_id: address.remote_id.clone().unwrap(),
            subject: message.subject,
            body,
        })
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

        let response = session
            .api()
            .create_draft(
                params,
                action,
                Default::default(),
                parent_id.map(Into::into),
            )
            .await?;
        Ok(response.message)
    }

    /// Apply an action which will create a new draft.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub async fn action_create_empty(
        queue: &Queue,
    ) -> Result<QueuedActionOutput<Create>, ActionError<Create>> {
        queue.queue_action(Create::empty()).await
    }

    /// Apply an action which will create reply draft with `reply_mode` to the
    /// message with `message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to execute.
    pub async fn action_create_reply(
        queue: &Queue,
        reply_mode: ReplyMode,
        message_id: LocalId,
    ) -> Result<QueuedActionOutput<Create>, ActionError<Create>> {
        queue
            .queue_action(Create::reply(reply_mode, message_id))
            .await
    }
}

/// Create new message for a sender with `address`.
///
/// Returns a newly initialized message and a body with a signature
/// associated with that address and the mail settings.
fn create_new_message(address: &Address, display_order: u64, body_len: usize) -> Message {
    let time = create_timestamp();

    Message {
        local_id: None,
        remote_id: None,
        local_conversation_id: None,
        remote_conversation_id: None,
        local_address_id: address.local_id.unwrap(),
        remote_address_id: address.remote_id.clone().unwrap(),
        attachments_metadata: vec![],
        cc_list: Default::default(),
        bcc_list: Default::default(),
        deleted: false,
        exclusive_location: None,
        expiration_time: 0,
        external_id: None,
        flags: Default::default(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![],
        num_attachments: 0,
        display_order,
        reply_tos: Default::default(),
        sender: MessageAddress {
            address: address.email.clone(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: address.display_name.clone(),
        },
        size: body_len as u64,
        snooze_time: 0,
        subject: DEFAULT_SUBJECT.to_owned(),
        time,
        to_list: Default::default(),
        unread: false,
        custom_labels: vec![],
        cached: false,
        row_id: None,
        stash: None,
    }
}

/// Create a new daft message based on `source_message` with `address` and
/// `reply_mode`.
///
/// `source_message` will be updated to reflect the reply status.
fn create_new_draft_with_reply_mode(
    source_message: &mut Message,
    reply_mode: ReplyMode,
    address: &Address,
    display_order: u64,
    body_len: usize,
) -> Message {
    let mut message = create_new_message(address, display_order, body_len);
    patch_message_with_reply_mode(&mut message, source_message, reply_mode);
    message
}

/// Copy all the data from the `source_message` into `message` taking
/// into account `reply_mode` of the draft.
fn patch_message_with_reply_mode(
    message: &mut Message,
    source_message: &mut Message,
    reply_mode: ReplyMode,
) {
    // Set conversation ids.
    message.local_conversation_id = source_message.local_conversation_id;
    message.remote_conversation_id = source_message.remote_conversation_id.clone();

    // Copy over the addresses based on reply mode
    match reply_mode {
        ReplyMode::Sender => {
            message.to_list = MessageAddresses {
                value: vec![source_message.sender.clone()],
            };
            message.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
            source_message.is_replied = true;
            source_message.flags |= MessageFlags::REPLIED;
        }
        ReplyMode::All => {
            message.to_list.value = vec![source_message.sender.clone()];
            message
                .to_list
                .value
                .extend_from_slice(&source_message.to_list.value);
            message.cc_list = source_message.cc_list.clone();
            message.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
            source_message.is_replied_all = true;
            source_message.flags |= MessageFlags::REPLIED_ALL;
        }
        ReplyMode::Forward => {
            message.subject = apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject);
            source_message.is_forwarded = true;
            source_message.flags |= MessageFlags::FORWARDED;
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
    let mut signature = if mail_settings.signature.is_empty() {
        address.signature.clone()
    } else if address.signature.is_empty() {
        mail_settings.signature.clone()
    } else {
        format!("{}\n\n{}", address.signature, mail_settings.signature)
    };

    if !signature.is_empty() {
        signature.insert_str(0, "\n\n");
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
    let pgp_provider = new_pgp_provider();
    let unlocked_keys = ctx.unlocked_address_keys(&pgp_provider, address_id).await?;
    let draft_body = DraftBody { body };
    draft_body
        .encrypt_draft_body(&pgp_provider, &unlocked_keys[0])
        .map_err(|e| {
            error!("Failed to encrypt draft: {e}");
            MailContextError::Crypto
        })
}

/// Store the message body in the cache.
fn store_body_in_cache<A>(
    cache: &ProtonCache<CacheMessageConfig>,
    message: &Message,
    body: &str,
    interface: &A,
) -> Result<(), AppError>
where
    A: Into<AgnosticInterface> + Interface,
{
    let key = CacheMessageKey::from_message(message, interface);

    cache.add_item(key, body.as_bytes()).map_err(|e| {
        error!("Failed to store draft body in cache: {e}");
        AppError::Cache(e)
    })?;
    Ok(())
}

/// Create a new timestamp.
fn create_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs()
}
/// Resolve the Drafts local label id.
async fn local_draft_label_id<A>(interface: &A) -> Result<LocalId, MailContextError>
where
    A: Into<AgnosticInterface> + Interface,
{
    let Some(local_draft_label_id) = LabelId::drafts().counterpart::<Label, _>(interface).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

pub const REPLY_PREFIX: &str = "Re: ";
pub const FORWARD_PREFIX: &str = "Fwd: ";

pub const DEFAULT_SUBJECT: &str = "(No Subject)";

fn apply_prefix_to_subject(prefix: &str, subject: &str) -> String {
    let trimmed_subject = subject.trim();
    if trimmed_subject.starts_with(prefix) {
        trimmed_subject.to_string()
    } else {
        format!("{prefix} {trimmed_subject}")
    }
}
