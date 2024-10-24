//! Models for the Proton Mail common library.
//!
//! This module contains the models used by the Proton Mail common library.
//! Models are data structures that can be saved in the database, and are used
//! to represent usable persistent data throughout the application. They are
//! distinctly different from any comparative structures used when interfacing
//! with the Proton API, which are used to represent data in transit only.
//!
//! Notably, the types in this module need to have [`Model`] applied, as they
//! should represent a record in a database table. All of their fields need to
//! be convertible to and from database-compatible format using [`ToSql`](stash::exports::ToSql)
//! and [`FromSql`](stash::exports::FromSql). They do not generally need to be
//! serializable or deserializable, as they are not used for network
//! communication or any other interchange purpose as a general requirement, and
//! so implementation of [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize)
//! is not necessary and may be a sign of a mistake. The exception here is for
//! child types, used by the models, for which these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`](stash::utils::sql_using_serde), as a
//! convenience mechanism. This is notably useful when wanting to store types as
//! JSON in a database field, for instance. However, child types should be
//! placed into the [`datatypes`](crate::datatypes) module, with only
//! first-order models being placed into this module.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need.
//!

mod conversation;
mod message;
mod network;
mod rollback_item;

mod draft;

#[cfg(test)]
#[path = "tests/models.rs"]
mod tests;

use crate::actions::{
    ConversationAction, ConversationAvailableActions, LabelAsAction, MessageAction,
    MessageAvailableActions, MoveAction, ReplyAction,
};
use crate::datatypes::{
    attachment, AlmostAllMail, AttachmentEncryptedSignature, AttachmentMetadata,
    AttachmentSignature, ComposerDirection, ComposerMode, ConversationCount, CustomLabel,
    Disposition, EncryptedMessageBody, ExclusiveLocation, KeyPackets, LabelColor, LabelType,
    MessageAddress, MessageAddresses, MessageAttachmentInfos, MessageButtons, MessageCount,
    MessageFlags, MimeType, MobileSettings, NextMessageOnMove, ParsedHeaders, PgpScheme,
    PmSignature, ShowImages, ShowMoved, SpamAction, SwipeAction, SystemLabel, SystemLabelId,
    ViewLayout, ViewMode,
};
use crate::find_in_query;
use crate::mailbox::decrypted_message::DecryptedMessageBody;
use crate::user_context::cache::{CacheMessageConfig, CacheMessageKey};
use crate::MailContextResult;
use crate::{AppError, MailUserContext, ALL_LABEL_TYPES};
use anyhow::{anyhow, Context};
use bytes::Bytes;
pub use draft::*;
use indoc::{formatdoc, indoc};
use itertools::Itertools;
use network::split_request;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::requests::{
    GetConversationsOptions, GetMessagesOptions, PatchLabelRequest, PostLabelsRequest,
    PutLabelRequest,
};
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, Conversation as ApiConversation,
    ConversationLabel as ApiConversationLabel, Label as ApiLabel, MailSettings as ApiMailSettings,
    Message as ApiMessage, MessageMetadata as ApiMessageMetadata, MessageMetadata, OperationResult,
};
use proton_api_mail::services::proton::responses::{
    GetAttachmentMetadataResponse, GetMessagesResponse,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::MAX_PAGE_ELEMENT_COUNT;
use proton_core_common::cache::{CacheError, CacheResult, ProtonCache};
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use proton_core_common::paginator::{DataSource, Paginator, Param};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, DecryptableAttachment,
    KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};
use proton_crypto_inbox::proton_crypto;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync as PgpProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_inbox::proton_crypto_inbox_mime::ProcessedMessage;
pub use rollback_item::RollbackItem;
use smart_default::SmartDefault;
use stash::exports::{SqliteError, ToSql};
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError, Tether};
use std::collections::btree_map::Entry;
use std::collections::hash_map::Entry as HmEntry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::future::Future;
use std::io::Read;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::vec;
use tracing::{debug, error, info, warn};

pub const MAIL_SETTINGS_ID: u64 = 1;

/// Represents a mail attachment.
///
/// The attachments are immutable on the server after creation and encrypted
/// with the address key of the message's address. While the type itself has
/// all the information we need to decrypt it, delivery to the application comes
/// in several steps that may or may not contain the full data.
///
/// A synchronized [`Attachment`] is an attachment which has all the fields
/// written to the database. [`AttachmentMetadata`] only contains partial
/// information necessary to identify the attachment and/or display some
/// context to the user.
///
/// # Lifecycle
///
/// 1. If the user has conversation view mode enabled, the first pieces
///    of metadata ([`AttachmentMetadata`]) arrive through the
///    [`Conversation`] type. If the view mode is message, go to 3.
///     1.1. The metadata is stored using [`Conversation::on_save()`]
///          method which ensures that it does not override a fully synchronized
///          [`Attachment`] and only updates the conversation local and remote id.
///     1.2. If no record for this attachment exists one is created.
/// 2. The user now opens the conversation, which sync the respective
///    [`Message`]s.
/// 3. [`Messages`] also contains [`AttachmentMetadata`] as well as the address
///    id for the key this attachment was encrypted with.
///     3.1 This is now stored with [`Message::on_save()`], which also
///         ensures it does not override a fully synced attachment and updates
///         the message ids and the address id.
///     3.2 If no attachment record exists, one is created.
/// 4. From 1 or 2, we can receive a request to fetch the full attachment.
///    At this stage we either have partial data from [`AttachmentMetadata`] or
///    a fully synchronized attachment.
///     4.1. We check witch is situation we are in with
///          [`has_complete_metadata()`].
///     4.2. If this returns false we need to sync the full attachment with
///          [`sync_complete_metadata()`].
///     4.3. If the check returns true, the attachment is ready for use.
/// 5. Finally, when fetching the message body ([`MessageBodyMetadata`]) we
///    receive the final bits of data regarding some headers and other metadata
///    used to display the attachment in web views.
///
/// Note: Extracting the last bit of information from [`MessageBodyMetadata`]
/// will come in a followup patch.
///
/// To ensure that we do not overwrite the [`Attachment`] data in the database
/// *NEVER* use [`Model::save()`] or [`Model::save_using()`] but instead
/// *ALWAYS* use [`Attachment::save()`] or [`Attachment::save_using()`].
///
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("attachments")]
pub struct Attachment {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// API Attachment id.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// Address with which this attachment was encrypted.
    #[DbField]
    pub local_address_id: Option<LocalId>,

    /// Address with which this attachment was encrypted. The address id can
    /// only be retrieved from a [`Message`] or the full [`Attachment`] type.
    #[DbField]
    pub remote_address_id: Option<RemoteId>,

    /// Local conversation id where this attachment is present.
    #[DbField]
    pub local_conversation_id: Option<LocalId>,

    /// Remote conversation id where this attachment is present.
    #[DbField]
    pub remote_conversation_id: Option<RemoteId>,

    /// Local message id where this attachment is present.
    #[DbField]
    pub local_message_id: Option<LocalId>,

    /// Remote message id where this attachment is present.
    #[DbField]
    pub remote_message_id: Option<RemoteId>,

    /// Attachment disposition.
    #[DbField]
    pub disposition: Disposition,

    /// TODO: Document this field.
    #[DbField]
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    /// TODO: Document this field.
    #[DbField]
    pub is_auto_forwardee: bool,

    /// TODO: Document this field.
    #[DbField]
    pub key_packets: Option<KeyPackets>,

    /// Mime type of the attachment
    #[DbField]
    pub mime_type: attachment::MimeType,

    /// File name of the attachment.
    #[DbField]
    pub filename: String,

    /// Sender of the attachment if received from an external address.
    #[DbField]
    pub sender: Option<MessageAddress>,

    /// TODO: Document this field.
    #[DbField]
    pub signature: Option<AttachmentSignature>,

    /// Size of the attachment in bytes.
    #[DbField]
    pub size: u64,

    /// True if this Attachment is cached
    #[DbField]
    pub cached: bool,

    #[DbField]
    /// Content id of the attachment if inlined in the message.
    pub content_id: Option<String>,

    #[DbField]
    /// Encoding of the attachment in the message.
    pub transfer_encoding: Option<String>,

    #[DbField]
    /// Custom proton width for this image. Yes, API is returning this as a string.
    pub image_width: Option<String>,

    /// Custom proton Height for this image. Yes, API is returning this as a string.
    #[DbField]
    pub image_height: Option<String>,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<AttachmentMetadata> for Attachment {
    fn from(value: AttachmentMetadata) -> Self {
        Self {
            local_id: value.local_id,
            remote_id: value.remote_id,
            local_address_id: None,
            remote_address_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_message_id: None,
            remote_message_id: None,
            disposition: value.disposition,
            enc_signature: None,
            is_auto_forwardee: false,
            key_packets: None,
            mime_type: value.mime_type,
            filename: value.filename,
            sender: None,
            signature: None,
            size: value.size,
            cached: false,
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
            row_id: None,
            stash: None,
        }
    }
}
impl From<Attachment> for AttachmentMetadata {
    fn from(value: Attachment) -> Self {
        Self {
            local_id: value.local_id,
            remote_id: value.remote_id,
            disposition: value.disposition,
            mime_type: value.mime_type,
            filename: value.filename,
            size: value.size,
        }
    }
}

impl Attachment {
    /// Load attachment metadata for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn load_conversation_attachment_metadata(
        conversation_id: LocalId,
        interface: &AgnosticInterface,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        Self::find("WHERE local_id IN (SELECT local_attachment_id FROM conversation_attachments WHERE local_conversation_id = ?)",
                params![conversation_id],
                   interface,
                   None
        )
        .await.map(|v| v.into_iter().map(Into::into).collect())
    }

    /// Load attachment metadata for a given `message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn load_message_attachment_metadata(
        message_id: LocalId,
        interface: &AgnosticInterface,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        Self::find("WHERE local_id IN (SELECT local_attachment_id FROM message_attachments WHERE local_message_id = ?)",
                   params![message_id],
                   interface,
                   None
        )
        .await.map(|v| v.into_iter().map(Into::into).collect())
    }

    /// Save or update the attachment in the database.
    ///
    /// It's imperative to call this function rather than [`Model::save()`] to
    /// make sure that we override the existing partial metadata rather than
    /// create a new entry that will cause a conflict.
    ///
    /// There is currently no way to handle this in stash directly, so we have
    /// to manually perform this check.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };
        self.save_using(&stash).await
    }

    /// Save or update the attachment in the database.
    ///
    /// It's imperative to call this function rather than
    /// [`Model::save_using()`] to make sure that we override the existing
    /// partial metadata rather than create a new entry that will cause a
    /// conflict.
    ///
    /// There is currently no way to handle this in stash directly, so we have
    /// to manually perform this check.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if self.local_id.is_none() {
            if let Some(remote_id) = self.remote_id.clone() {
                if let Some(existing) =
                    Self::find_first("WHERE remote_id=?", params![remote_id], interface).await?
                {
                    self.local_id = existing.local_id;
                    self.row_id = existing.row_id;
                }
            }
        }
        if self.local_address_id.is_none() {
            if let Some(remote_address_id) = self.remote_address_id.clone() {
                self.local_address_id = remote_address_id
                    .counterpart::<Address, _>(interface)
                    .await?;
            }
        }

        if self.local_message_id.is_none() {
            if let Some(remote_message_id) = self.remote_message_id.clone() {
                self.local_message_id = remote_message_id
                    .counterpart::<Message, _>(interface)
                    .await?;
            }
        }

        if self.local_conversation_id.is_none() {
            if let Some(remote_conversation_id) = self.remote_conversation_id.clone() {
                self.local_conversation_id = remote_conversation_id
                    .counterpart::<Conversation, _>(interface)
                    .await?;
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Fetch attachment content from the API.
    ///
    /// Calls the API to load encrypted attachment content for the given
    /// attachment.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Attachment).
    ///
    /// # Parameters
    ///
    /// * `id`  - The ID of the attachment to fetch.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_content<PM: ProtonMail>(
        id: RemoteId,
        api: &PM,
    ) -> Result<Bytes, ApiServiceError> {
        api.get_attachment(id.into()).await
    }

    /// Fetch attachment metadata from the API.
    ///
    /// Calls the API to load the full attachment metadata for decrypting its
    /// content.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Attachment).
    ///
    /// # Parameters
    ///
    /// * `id`  - The ID of the attachment to fetch.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_metadata<PM: ProtonMail>(
        id: RemoteId,
        api: &PM,
    ) -> Result<GetAttachmentMetadataResponse, ApiServiceError> {
        api.get_attachment_metadata(id.into()).await
    }

    /// Check whether attachment is complete.
    ///
    /// Attachment metadata is considered complete when all the information
    /// required to decrypt the attachment is in the database. When storing
    /// conversation/messages into the database we only get partial data for the
    /// attachment.
    ///
    /// To complete the data, one needs to provide the full metadata.
    ///
    pub fn has_complete_metadata(&self) -> bool {
        self.key_packets.is_some() && self.remote_address_id.is_some()
    }

    /// Synchronize the full attachment metadata for the attachment.
    ///
    /// The database might contain partial attachment metadata missing the
    /// relevant information for decryption. To synchronize the full attachment
    /// metadata this method must be called.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_complete_metadata<PM: ProtonMail>(
        &mut self,
        api: &PM,
        interface: &AgnosticInterface,
    ) -> Result<Option<()>, AppError> {
        let remote_attachment_id = if let Some(remote_id) = self.remote_id.clone() {
            remote_id
        } else {
            return Err(StashError::IdNotSet.into());
        };
        let mut attachment = Self::from(
            Self::fetch_metadata(remote_attachment_id, api)
                .await?
                .attachment,
        );
        attachment.local_id = self.local_id;
        attachment.row_id = self.row_id;
        attachment.save_using(interface).await?;
        *self = attachment;
        Ok(Some(()))
    }

    /// This function syncs the message attachments headers detailed in step 5
    /// of the documentation of [`Attachment`] for the given `message`.
    ///
    /// This is the last piece of metadata which completes the attachment type.
    ///
    /// # Error
    ///
    /// Return error if the query failed.
    pub async fn update_headers_from_api_message<A>(
        message: &ApiMessage,
        interface: &A,
    ) -> Result<Vec<Self>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut result = Vec::with_capacity(message.attachments.len());

        for message_attachment in &message.attachments {
            let remote_id: RemoteId = message_attachment.id.clone().into();
            let Some(mut attachment) = Attachment::find_by_id(remote_id, interface).await? else {
                if message_attachment.disposition
                    != proton_api_mail::services::proton::response_data::Disposition::Inline
                {
                    return Err(AppError::UnknownAttachment(
                        message_attachment.id.clone().into(),
                    ));
                }

                // If it's an inline attachment it's the first time we are
                // seeing this data.
                let mut new_attachment = Attachment {
                    local_id: None,
                    remote_id: Some(message_attachment.id.clone().into()),
                    local_address_id: None,
                    remote_address_id: Some(message.metadata.address_id.clone().into()),
                    local_conversation_id: None,
                    remote_conversation_id: Some(message.metadata.conversation_id.clone().into()),
                    local_message_id: None,
                    remote_message_id: Some(message.metadata.id.clone().into()),
                    disposition: message_attachment.disposition.into(),
                    enc_signature: message_attachment.enc_signature.clone().map(Into::into),
                    is_auto_forwardee: false,
                    key_packets: Some(message_attachment.key_packets.clone().into()),
                    mime_type: attachment::MimeType::from_str(&message_attachment.mime_type)?,
                    filename: message_attachment.name.clone(),
                    sender: Some(message.metadata.sender.clone().into()),
                    signature: message_attachment.signature.clone().map(Into::into),
                    size: message_attachment.size,
                    cached: false,
                    content_id: message_attachment.headers.content_id.clone(),
                    transfer_encoding: message_attachment.headers.content_transfer_encoding.clone(),
                    image_width: message_attachment.headers.image_width.clone(),
                    image_height: message_attachment.headers.image_height.clone(),
                    row_id: None,
                    stash: None,
                };
                new_attachment
                    .save_using(interface)
                    .await
                    .inspect_err(|e| {
                        error!(
                            "Failed to save new inline attachment {}:{e}",
                            message_attachment.id
                        )
                    })?;
                result.push(new_attachment);
                continue;
            };

            attachment.content_id = message_attachment.headers.content_id.clone();
            attachment.transfer_encoding =
                message_attachment.headers.content_transfer_encoding.clone();
            attachment.image_width = message_attachment.headers.image_width.clone();
            attachment.image_height = message_attachment.headers.image_height.clone();
            attachment.signature = message_attachment.signature.clone().map(Into::into);
            attachment.key_packets = Some(message_attachment.key_packets.clone().into());
            attachment.enc_signature = message_attachment.enc_signature.clone().map(Into::into);
            attachment.disposition = message_attachment.disposition.into();
            attachment.save_using(interface).await?;
            result.push(attachment);
        }

        Ok(result)
    }

    /// Get all attachments for a given message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn for_message<A>(
        local_message_id: LocalId,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Attachment::find(
            indoc! {"
            WHERE local_id IN (
                SELECT local_attachment_id FROM message_attachments
                WHERE local_message_id=?
            )
        "},
            params![local_message_id],
            interface,
            None,
        )
        .await
    }
}

// TODO: The use of the "Real" wrappers is because the source types don't
// TODO: implement the traits we need. At a later date we should implement those
// TODO: traits directly on the source types, and remove these wrappers.
impl DecryptableAttachment for Attachment {
    fn attachment_key_packets(&self) -> &RealKeyPackets {
        self.key_packets
            .as_ref()
            .expect("Should exist at this point")
    }

    fn attachment_signature(&self) -> Option<&RealAttachmentSignature> {
        self.signature.as_deref()
    }

    fn attachment_encrypted_signature(&self) -> Option<&RealAttachmentEncryptedSignature> {
        self.enc_signature.as_deref()
    }
}

impl From<ApiAttachment> for Attachment {
    fn from(value: ApiAttachment) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            local_address_id: None,
            remote_address_id: Some(value.address_id.into()),
            local_conversation_id: None,
            remote_conversation_id: Some(value.conversation_id.into()),
            local_message_id: None,
            remote_message_id: Some(value.message_id.into()),
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.clone().map(|v| v.into()),
            is_auto_forwardee: value.is_auto_forwardee,
            key_packets: Some(value.key_packets.clone().into()),
            mime_type: value.mime_type.parse().unwrap_or_default(),
            filename: value.name,
            sender: value.sender.map(|v| v.into()),
            signature: value.signature.map(|v| v.into()),
            size: value.size,
            cached: false,
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("conversations")]
#[ModelActions(on_load, on_save)]
pub struct Conversation {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub attachment_info: MessageAttachmentInfos,

    /// Attachment metadata associated with this conversation.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[DbField]
    pub deleted: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_snooze_reminder: bool,

    /// Exclusive location of the [`Conversation`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

    /// TODO: Document this field.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    pub labels: Vec<ConversationLabel>,

    /// TODO: Document this field
    #[DbField]
    pub num_attachments: u64,

    /// How many messages there are in the conversation.
    #[DbField]
    pub num_messages: u64,

    /// How many unread messages there are in the conversation.
    #[DbField]
    pub num_unread: u64,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u64,

    #[DbField]
    /// TODO: Document this field.
    pub recipients: MessageAddresses,

    #[DbField]
    /// TODO: Document this field.
    pub senders: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub size: u64,

    /// TODO: Document this field.
    #[DbField]
    pub subject: String,

    /// Whether this conversation is fully known.
    ///
    /// When in message view mode we need to be able to create messages
    /// without their conversation counterpart. We create an unknown conversation
    /// entry.
    ///
    /// As it is expensive to sync the conversation, we need to defer this until
    /// we either retrieve the conversation from the server or one of the
    /// events creates it for us.
    #[DbField]
    pub is_known: bool,

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// Whether the conversation has synced its messages.
    #[DbField]
    pub has_messages: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Conversation {
    /// Create a new unknown conversation where we only know the `remote_id`.
    ///
    /// See [`Conversation::is_known`] for more details.
    pub fn unknown(remote_id: RemoteId) -> Self {
        Self {
            local_id: None,
            remote_id: Some(remote_id),
            attachment_info: Default::default(),
            attachments_metadata: vec![],
            deleted: false,
            display_snooze_reminder: false,
            exclusive_location: None,
            expiration_time: 0,
            labels: vec![],
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            display_order: 0,
            recipients: Default::default(),
            senders: Default::default(),
            size: 0,
            subject: "".to_string(),
            is_known: false,
            custom_labels: vec![],
            row_id: None,
            stash: None,
            has_messages: false,
        }
    }

    /// Save a conversation to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a message to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
                self.stash = existing.stash;
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - Id of the label to assign
    /// * `ids`         - The IDs of the conversations to label.
    /// * `interface`   - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn apply_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let message_ids = interface
                .query_values::<_, LocalId>(
                    indoc! {"
            WITH conv_msgs AS (
                SELECT local_id,? AS label_id FROM messages WHERE local_conversation_id=?
            )
            INSERT OR IGNORE INTO
                message_labels (local_message_id, local_label_id)
            SELECT * FROM conv_msgs RETURNING local_message_id AS value
"},
                    params![label_id, id],
                )
                .await?;

            if !message_ids.is_empty() {
                Conversation::label_impl(label_id, id, &message_ids, interface).await?
            } else {
                // Fallback without message metadata. We should grab the highest time values from
                // all the remaining labels assigned to this conversation. All conversations
                // messages will always have the All Mail label assigned.
                if ConversationLabel::find_first(
                    "WHERE local_conversation_id=? AND local_label_id=?",
                    params![id, label_id],
                    interface,
                )
                .await?
                .is_none()
                {
                    let Some(mut label) = Label::find_by_id(label_id, interface).await? else {
                        return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
                    };

                    let mut new_label = ConversationLabel {
                        local_id: None,
                        local_conversation_id: Some(id),
                        local_label_id: Some(id),
                        remote_label_id: label.remote_id.clone(),
                        context_expiration_time: 0,
                        context_num_attachments: 0,
                        context_num_messages: 0,
                        context_num_unread: 0,
                        context_size: 0,
                        context_snooze_time: 0,
                        context_time: 0,
                        deleted: false,
                        row_id: None,
                        stash: None,
                    };
                    let conversation_labels = ConversationLabel::find(
                        "WHERE local_conversation_id=?",
                        params![id],
                        interface,
                        None,
                    )
                    .await?;
                    for conversation_label in conversation_labels {
                        new_label.context_expiration_time = conversation_label
                            .context_expiration_time
                            .max(new_label.context_expiration_time);
                        new_label.context_num_attachments = conversation_label
                            .context_num_attachments
                            .max(new_label.context_num_attachments);
                        new_label.context_num_messages = conversation_label
                            .context_num_messages
                            .max(new_label.context_num_messages);
                        new_label.context_num_unread = conversation_label
                            .context_num_unread
                            .max(new_label.context_num_unread);
                        new_label.context_size =
                            conversation_label.context_size.max(new_label.context_size);
                        new_label.context_snooze_time = conversation_label
                            .context_snooze_time
                            .max(new_label.context_snooze_time);
                        new_label.context_time =
                            conversation_label.context_time.max(new_label.context_time);
                    }

                    new_label.save_using(interface).await?;

                    label.total_conv += 1;
                    label.save_using(interface).await?;
                }
            }
        }

        Ok(())
    }

    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The ID of the label to apply to the conversations.
    /// * `ids`         - The IDs of the conversations to unlabel.
    /// * `spam_action` - TODO: Document this parameter.
    /// * `api`         - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn apply_label_to_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<RemoteId>,
        spam_action: Option<bool>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_label(ids, label_id.into(), spam_action)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversations` - TODO: Document this parameter.
    /// * `interface`     - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn create_or_update_conversations<A>(
        conversations: Vec<Conversation>,
        interface: &A,
    ) -> Result<Vec<LocalId>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            Self::save_using(&mut conv, interface).await?;
            ids.push(conv.local_id.unwrap());
        }

        Ok(ids)
    }

    /// Mark conversations as deleted.
    ///
    /// Note that this is a soft delete. Conversations are only
    /// really deleted when the event loop sends the delete event.
    ///
    /// Finally, only the messages in the active label will be marked as deleted
    /// unless the label is AllMail which will mark all messages in all labels as deleted.
    /// moreover the conversation will be removed from all labels as well as deleted field will
    /// be set to true.
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_deleted<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = SystemLabel::AllMail.local_id(interface).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_deleted_all_mail(ids, interface).await?;
        } else {
            Self::mark_deleted_current_label(label_id, ids, interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_deleted`].
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_deleted_all_mail<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_by_id(id, interface).await? else {
                continue;
            };

            conversation.deleted = true;
            conversation.num_unread = 0;
            conversation.num_messages = 0;
            conversation.num_attachments = 0;
            conversation.size = 0;
            conversation.save_using(interface).await?;

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 0
               "},
                params![id],
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let stats = Message::update_message_counters_after_soft_delete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                conversation
                    .remove_conversation_from_all_labels(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after soft delete of conversation in active view `AllMail`.
    ///
    /// # Parameters
    ///
    /// * `all_stats`  - The stats of the messages that were deleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn remove_conversation_from_all_labels<A>(
        &self,
        all_stats: HashMap<LocalId, MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            let stats = all_stats.get(&label_id);

            label.total_conv -= 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv -= 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = true;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted in active label.
    /// More information can be found in [`Conversation::mark_deleted`].
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_deleted_current_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_first(
                "WHERE local_id=? AND deleted=0 AND is_known=1",
                params![id],
                interface,
            )
            .await?
            else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 0 AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
               "},
                params![id, label_id],
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let all_stats = Message::update_message_counters_after_soft_delete(
                    messages.into_iter(),
                    interface,
                )
                .await?;

                let stats = all_stats.get(&label_id);

                conversation
                    .mark_delete_update_stats(stats, interface)
                    .await?;

                conversation
                    .remove_conversation_from_label(label_id, stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates active label counters after soft delete of conversation.
    ///
    /// # Parameters
    ///
    /// * `label_id`   - The ID of the label to update.
    /// * `all_stats`  - The stats of the messages that were deleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn remove_conversation_from_label<A>(
        &mut self,
        label_id: LocalId,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=0 AND local_label_id=?",
            params![self.local_id.unwrap(), label_id],
            interface,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            label.total_conv -= 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv -= 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = true;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted.
    ///
    /// Only the messages in the active label will be marked as undeleted
    /// unless the label is AllMail which will mark all messages in all labels as undeleted.
    /// moreover the conversation will be assigned to all labels as well as deleted field will
    /// be set to false.
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_undeleted<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = SystemLabel::AllMail.local_id(interface).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_undeleted_all_mail(ids, interface).await?;
        } else {
            Self::mark_undeleted_current_label(label_id, ids, interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the conversations to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_all_mail<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_by_id(id, interface).await? else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 1
               "},
                params![id],
                interface,
                None,
            )
            .await?;

            let mut count = 0;
            let mut unread_count = 0;
            let mut attachment_count = 0;
            let mut size = 0;

            for message in &mut messages {
                message.deleted = false;
                count += 1;
                unread_count += message.unread as u64;
                attachment_count += message.num_attachments as u64;
                size += message.size;

                message.save_using(interface).await?
            }

            conversation.deleted = false;
            conversation.num_messages += count;
            conversation.num_unread += unread_count;
            conversation.num_attachments += attachment_count;
            conversation.size += size;

            conversation.save_using(interface).await?;

            if !messages.is_empty() {
                let stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                conversation
                    .add_conversation_to_all_labels(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after undelete of conversation in active view `AllMail`.
    ///
    /// # Parameters
    ///
    /// * `all_stats`  - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn add_conversation_to_all_labels<A>(
        &self,
        all_stats: HashMap<LocalId, MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=1",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            let stats = all_stats.get(&label_id);

            label.total_conv += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv += 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = false;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted in active label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_current_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) =
                Conversation::find_first("WHERE local_id=? AND is_known=1", params![id], interface)
                    .await?
            else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 1 AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
               "},
                params![id, label_id],
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = false;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let all_stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                let stats = all_stats.get(&label_id);

                conversation
                    .add_conversation_to_label(label_id, stats, interface)
                    .await?;

                conversation
                    .mark_undelete_update_stats(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates active label counters after undelete of conversation.
    ///
    /// # Parameters
    ///
    /// * `label_id`   - The ID of the label to update.
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn add_conversation_to_label<A>(
        &mut self,
        label_id: LocalId,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=1 AND local_label_id=?",
            params![self.local_id.unwrap(), label_id],
            interface,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            label.total_conv += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv += 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = false;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }
    /// Updates conversation counters after delete of conversation.
    ///
    /// # Parameters
    ///
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn mark_delete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let undeleted_messages = Message::count(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.local_id],
            interface,
        )
        .await?;

        if undeleted_messages == 0 {
            self.deleted = true;
        }

        if let Some(stats) = stats {
            self.num_messages = self.num_messages.saturating_sub(stats.count);
            self.num_unread = self.num_unread.saturating_sub(stats.unread_count);
            self.num_attachments = self.num_attachments.saturating_sub(stats.attachment_count);
            self.size = self.size.saturating_sub(stats.size);
        }

        self.save_using(interface).await?;

        Ok(())
    }

    /// Updates conversation counters after undelete of conversation.
    ///
    /// # Parameters
    ///
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn mark_undelete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.num_messages += stats.count;
            self.num_unread += stats.unread_count;
            self.num_attachments += stats.attachment_count;
            self.size += stats.size;
            self.deleted = false;
            self.save_using(interface).await?;
        }

        Ok(())
    }

    /// Delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn delete_multiple_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Get the conversation counts.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_counts<PM: ProtonMail>(
        api: &PM,
    ) -> Result<Vec<ConversationCount>, ApiServiceError> {
        api.get_conversations_count()
            .await
            .map(|r| r.counts.into_iter().map(|c| c.into()).collect())
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Parameters
    ///
    /// * `local_id` - local ID of the conversation.
    /// * `label`    - label model from where the conversation is being viewed.
    /// * `messages` - Array of message models for the conversation.
    ///
    /// # Errors
    ///
    /// When unable to pick the message for the conversation in the current view.
    ///
    pub fn message_id_to_open(
        local_id: LocalId,
        label: &Label,
        messages: &[Message],
    ) -> Result<LocalId, AppError> {
        if messages.is_empty() {
            return Err(AppError::ConversationHasNoMessages(local_id));
        }
        // If we fail to find any message, return the last message in the list.
        Ok(Self::first_unread_message(label, messages)
            .unwrap_or(messages.last().unwrap().local_id.unwrap()))
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Parameters
    ///
    /// * `label`    - label model from where the conversation is being viewed.
    /// * `messages` - Array of message models for the conversation.
    ///
    pub fn first_unread_message(label: &Label, messages: &[Message]) -> Option<LocalId> {
        if messages.is_empty() {
            return None;
        }

        fn first_consecutive_unread_msg(
            label_id: &LabelId,
            messages: &[Message],
            filter: impl Fn(&Message) -> bool,
        ) -> Option<LocalId> {
            let mut last_unread = None;

            for msg in messages.iter().rev() {
                if msg.unread && filter(msg) {
                    last_unread.clone_from(&msg.local_id);
                } else if last_unread.is_some() {
                    break;
                }
            }

            last_unread.or_else(|| {
                messages
                    .iter()
                    .rev()
                    .find(|m| filter(m) && m.label_ids.contains(label_id))
                    .and_then(|m| m.local_id)
            })
        }

        let view_is_starred_label_or_folder = label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id == Some(LabelId::starred());
        let label_id = label.remote_id.as_ref()?;

        if view_is_starred_label_or_folder {
            first_consecutive_unread_msg(label_id, messages, |msg| !msg.flags.is_draft())
        } else {
            first_consecutive_unread_msg(label_id, messages, |msg| {
                !(msg.flags.is_draft() || msg.flags.is_sent_auto())
            })
        }
    }

    /// TODO: Document this method.
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.labels
            .iter()
            .any(|l| l.remote_label_id == Some(LabelId::starred()))
    }

    /// Load all models::Label for `self` models::ConversationLabel list.
    ///
    /// # Errors
    ///
    /// Database error.
    ///
    pub async fn load_labels<A>(&self, interface: &A) -> Result<Vec<Label>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let ids = self
            .labels
            .iter()
            .filter_map(|label| label.local_label_id)
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect_vec();

        let labels = Label::find(
            format!(
                "WHERE local_id IN ({}) ORDER BY display_order ASC",
                vec!["?"; ids.len()].join(",")
            ),
            ids,
            interface,
            None,
        )
        .await?;

        Ok(labels)
    }

    /// Extends [`Model::load()`] to pre-load child records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        self.labels = ConversationLabel::find(
            "WHERE local_conversation_id = ?",
            params![self.local_id],
            interface,
            None,
        )
        .await?;
        let labels = self.load_labels(interface).await?;
        self.exclusive_location = ExclusiveLocation::from_labels(&labels);
        self.attachments_metadata =
            Attachment::load_conversation_attachment_metadata(self.local_id.unwrap(), interface)
                .await?;
        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        // Example... not good to do this here, though, as the total number comes
        // from the API.
        // self.num_messages = stash.query::<_, QueryResultU64>(
        //     "SELECT COUNT(*) as value FROM messages WHERE local_conversation_id = ?",
        //     params![self.local_id],
        // ).await?.into_iter().next().unwrap().value;

        Ok(())
    }

    /// Extends [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this conversation.
        if !self.labels.is_empty() {
            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                    AND remote_label_id NOT IN ({})
                ",
                        vec!["?"; self.labels.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(self.labels.iter().map(|label| {
                            Box::new(label.remote_label_id.clone()) as Box<dyn ToSql + Send>
                        }))
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        if !self.attachments_metadata.is_empty() {
            let local_ids = {
                // Create attachment from partial metadata present in a conversation.
                // If attachment record already exists, the conversation ids are updated.
                // If no record exists we create a new one.
                let mut result = Vec::with_capacity(self.attachments_metadata.len());
                for metadata in &self.attachments_metadata {
                    let mut attachment = Attachment::find_first(
                        "WHERE remote_id = ?",
                        params![metadata.remote_id.clone()],
                        interface,
                    )
                    .await?
                    .unwrap_or(Attachment::from(metadata.clone()));

                    attachment.local_conversation_id = self.local_id;
                    attachment.remote_conversation_id = self.remote_id.clone();
                    attachment.save_using(interface).await?;

                    let local_id = attachment.local_id.expect("Should be set");

                    interface
                        .execute(
                            "INSERT OR IGNORE INTO conversation_attachments VALUES (?,?)",
                            params![self.local_id.unwrap(), local_id],
                        )
                        .await?;

                    result.push(local_id);
                }

                result
            };

            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                    AND local_attachment_id NOT IN ({})
                ",
                        vec!["?"; local_ids.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(
                            local_ids
                                .into_iter()
                                .map(|attachment| Box::new(attachment) as Box<dyn ToSql + Send>),
                        )
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        for label in &mut self.labels {
            label.local_conversation_id = self.local_id;
            label.save_using(interface).await?
        }
        Ok(())
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as read.
    /// * `tether` - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_read<A>(
        conversation_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for conversation_id in conversation_ids {
            let mut conversation = Conversation::find_by_id(conversation_id, interface)
                .await?
                .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;
            // If conversation has no unread messages, there is nothing to do.
            if conversation.num_unread == 0 {
                continue;
            }

            // Update conversation unread count.
            conversation.num_unread = 0;
            conversation.save_using(interface).await?;

            // Update conversation labels unread stats.
            let conversation_labels = ConversationLabel::find(
                "WHERE local_conversation_id=? AND context_num_unread <> 0",
                params![conversation_id],
                interface,
                None,
            )
            .await?;

            let mut label_counts = HashMap::new();
            for mut conversation_label in conversation_labels {
                match label_counts.entry(conversation_label.local_label_id.unwrap()) {
                    HmEntry::Occupied(mut o) => {
                        *o.get_mut() += 1;
                    }
                    HmEntry::Vacant(v) => {
                        v.insert(1);
                    }
                }

                conversation_label.context_num_unread = 0;
                conversation_label.save_using(interface).await?
            }

            for (label_id, count) in &mut label_counts {
                if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                    label.unread_conv -= *count;
                    label.save_using(interface).await?
                }

                // reset for messages.
                *count = 0;
            }

            // Update messages
            let messages = Message::find(
                "WHERE local_conversation_id=? AND unread<>0",
                params![conversation_id],
                interface,
                None,
            )
            .await?;

            for mut message in messages {
                let local_message_id = message.local_id.unwrap();
                message.unread = false;
                message.save_using(interface).await?;

                let label_ids = interface.query_values::<_, LocalId>("SELECT local_label_id AS value FROM message_labels WHERE local_message_id=?", params![local_message_id]).await?;
                for label_id in label_ids {
                    match label_counts.entry(label_id) {
                        HmEntry::Occupied(mut o) => {
                            *o.get_mut() += 1;
                        }
                        HmEntry::Vacant(v) => {
                            v.insert(1);
                        }
                    }
                }
            }

            // update message label counters
            for (label_id, count) in &mut label_counts {
                if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                    label.unread_msg -= *count;
                    label.save_using(interface).await?
                }
            }
        }

        Ok(())
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs of the conversations to mark as read.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_read_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| async {
            api.put_conversations_read(ids).await.map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Mark multiple conversations as unread.
    /// For each conversation only the last read message gets marked as unread.
    ///
    /// # Parameters
    ///
    /// * `local_label_id`  - Label id where the operation is being applied.
    /// * `ids`             - The IDs of the conversations to mark as unread.
    /// * `tether`          - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_unread(
        local_label_id: LocalId,
        conversation_ids: impl IntoIterator<Item = LocalId>,
        tether: &Tether,
    ) -> Result<(), StashError> {
        for conversation_id in conversation_ids {
            let Some(mut conversation) = Conversation::find_by_id(conversation_id, tether).await?
            else {
                warn!("Conversation with id {conversation_id} does not exist!");
                continue;
            };
            // Find all messages that need to be marked as read.
            let message = Message::find_first(
                "WHERE local_conversation_id=?
                AND unread=0
                ORDER BY time",
                params![conversation_id],
                tether,
            )
            .await?;

            let total_conversation_message_count = tether
                .query_value::<_, u64>(
                    "SELECT COUNT(local_id) AS value FROM messages WHERE local_conversation_id=?",
                    params![conversation_id],
                )
                .await?;

            let Some(mut message) = message else {
                if total_conversation_message_count == 0 {
                    // These conversations where asked to be marked as read, but had
                    // no messages. Either the messages were already mark as read or
                    // there was no metadata. For these we need to set the unread
                    // count to 1 and update the current label count. We let the
                    // event loop take care of the rest.

                    let conv_labels = ConversationLabel::find(
                        "WHERE local_conversation_id=? AND local_label_id=?",
                        params![conversation_id, local_label_id],
                        tether,
                        None,
                    )
                    .await?;
                    for mut conv_label in conv_labels {
                        conv_label.context_num_unread += 1;
                        conv_label.save_using(tether).await?;
                    }

                    conversation.num_unread += 1;
                    conversation.save_using(tether).await?;

                    if let Some(mut label) = Label::find_by_id(local_label_id, tether).await? {
                        label.unread_conv += 1;
                        label.save_using(tether).await?;
                    }
                }
                continue;
            };

            // Update the message

            message.unread = true;
            message.save_using(tether).await?;

            // Update the label counts

            let label_ids = tether
                .query_values::<_, LocalId>(
                    "SELECT local_label_id AS value
                     FROM message_labels
                     WHERE local_message_id=?",
                    params![message.id_value()?],
                )
                .await?;

            for label_id in label_ids {
                if let Some(mut label) = Label::find_by_id(label_id, tether).await? {
                    // Always update the message count
                    label.unread_msg += 1;
                    // only update conversation unread count if we really marked
                    // all messages as unread. If we have mixture, this value
                    // should not be modified
                    if total_conversation_message_count == 1 {
                        label.unread_conv += 1;
                    }

                    label.save_using(tether).await?;
                }

                if let Some(mut conv_label) = ConversationLabel::find_first(
                    "WHERE local_label_id=?",
                    params![label_id],
                    tether,
                )
                .await?
                {
                    conv_label.context_num_unread += 1;
                    conv_label.save_using(tether).await?;
                }
            }

            // update conversations
            conversation.num_unread += 1;
            conversation.save_using(tether).await?;
        }
        Ok(())
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs of the conversations to mark as unread.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_unread_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| async {
            api.put_conversations_unread(ids).await.map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - Id of the label to remove.
    /// * `ids`         - The IDs of the conversations to unlabel.
    /// * `interface`   - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn remove_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut label = Label::find_by_id(label_id, interface)
            .await?
            .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;

        for id in ids {
            // Remove label from messages
            let message_ids = interface
                .query_values::<_, LocalId>(
                    indoc! {"
                    DELETE FROM message_labels
                    WHERE local_message_id IN (
                        SELECT local_id FROM messages WHERE local_conversation_id=?1
                    ) AND message_labels.local_label_id=?2
                    RETURNING local_message_id AS value
                    "},
                    params![id, label_id],
                )
                .await?;

            // We can only do this part if we have conversation metadata.
            if !message_ids.is_empty() {
                let num_unread = Message::find(
                    format!(
                        "WHERE local_id IN ({})",
                        vec!["?"; message_ids.len()].join(",")
                    ),
                    message_ids
                        .iter()
                        .map(|&v| -> Box<dyn ToSql + Send> { Box::new(*v) })
                        .collect(),
                    interface,
                    None,
                )
                .await?
                .into_iter()
                .fold(0_u64, |mut value, message| {
                    if message.unread {
                        value += 1;
                    }
                    value
                });

                label.total_msg -= message_ids.len() as u64;
                label.unread_msg -= num_unread;
            }

            // Remove conversation label
            match interface
                .query_value::<_, u64>(
                    indoc! {"
                    DELETE FROM conversation_labels
                    WHERE local_conversation_id=? AND local_label_id=?
                    RETURNING context_num_unread AS value
                    "},
                    params![id, label_id],
                )
                .await
            {
                Ok(num_unread) => {
                    if num_unread > 0 {
                        label.unread_conv -= 1;
                    }
                    label.total_conv -= 1;
                }
                Err(e) => {
                    if !matches!(
                        e,
                        StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                    ) {
                        return Err(e);
                    }
                }
            }
        }

        label.save_using(interface).await?;
        Ok(())
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to apply to the conversations.
    /// * `ids`      - The IDs of the conversations to unlabel.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn remove_label_from_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_unlabel(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Given a list of conversations check if there are any missing dependencies like undownloaded
    /// labels.
    ///
    ///
    /// # Parameters
    ///
    /// * `conversations` - The conversations to check.
    /// * `api`           - The API instance to use.
    /// * `stash`         - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    async fn sync_dependencies<A>(
        conversations: &[ApiConversation],
        api: &Proton,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut missing_labels = vec![];
        for conv in conversations {
            for label in &conv.labels {
                let rid: RemoteId = label.id.clone().into();
                if (Label::find_by_id(rid, interface)).await?.is_none() {
                    missing_labels.push(label.id.clone());
                }
            }
        }

        if !missing_labels.is_empty() {
            info!(
                "{} label(s) were in a conversations but not locally, synchronizing...",
                missing_labels.len()
            );
            Label::sync_labels_by_ids(api, interface, missing_labels).await?;
        }
        Ok(())
    }
    /// Search for conversations.
    ///
    /// This function accepts search options and calls the API to find any
    /// conversations that fit the criteria. It operates globally and is not
    /// based on a particular mailbox; this restriction can be applied via the
    /// options.
    ///
    /// # Parameters
    ///
    /// * `options` - The search options to use.
    /// * `api`     - The API instance to use.
    /// * `stash`   - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database. Can also return an error if a found
    /// conversation cannot be loaded, although this would indicate a
    /// significant problem.
    ///
    pub async fn search(
        options: GetConversationsOptions,
        api: &Proton,
        stash: &Stash,
    ) -> Result<Vec<Conversation>, AppError> {
        // Fetch all the conversations from the API
        let conversations = api
            .get_conversations(options)
            .await
            .context("Error fetching the conversations from the API")?
            .conversations;

        Self::sync_dependencies(&conversations, api, stash).await?;

        let mut conversations = conversations
            .into_iter()
            .map(Conversation::from)
            .collect_vec();
        Self::create_or_update_conversations(conversations.clone(), stash).await?;
        conversations.sort_unstable_by(|x, y| x.display_order.cmp(&y.display_order).reverse());

        Ok(conversations)
    }

    /// Star multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as starred.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn star_multiple(ids: Vec<LocalId>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_id(RemoteId::from(LabelId::starred()), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::apply_label(label_id, ids, &stash.connection()).await
    }

    /// Unstar multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as starred.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn unstar_multiple(ids: Vec<LocalId>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_id(RemoteId::from(LabelId::starred()), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::remove_label(label_id, ids, &stash.connection()).await
    }

    /// Synchronize the conversations and message counts for each label.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_conversation_and_message_counts<PM: ProtonMail>(
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let (conversation_counts, message_counts) =
            futures::join!(Conversation::fetch_counts(api), Message::fetch_counts(api));
        let (conversation_counts, message_counts) = (conversation_counts?, message_counts?);

        let tx = stash.transaction().await?;
        Label::create_or_update_conversation_counts(conversation_counts, &tx).await?;
        Label::create_or_update_message_counts(message_counts, &tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Synchronize the first `count` conversations of the label with `label_id`.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to sync.
    /// * `count`    - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_first_conversation_page<PM: ProtonMail>(
        label_id: LabelId,
        count: usize,
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let response = api
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(label_id.into()),
                page: 0,
                page_size: count.min(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        let tx = stash.transaction().await?;
        Self::create_or_update_conversations(
            response
                .conversations
                .into_iter()
                .map(Conversation::from)
                .collect(),
            &tx,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Undelete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to undelete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn undelete_multiple(
        ids: Vec<LocalId>,
        label_id: LocalId,
        tether: &Tether,
    ) -> Result<usize, StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        tether
            .execute(
                formatdoc!(
                    r"
            UPDATE
                messages
            SET
                deleted = 0
            WHERE
                local_conversation_id IN ({})
                AND deleted = 1
                AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
                ",
                    ids.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                params![label_id],
            )
            .await
    }

    /// Undelete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to undelete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn undelete_multiple_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Remove all removable labels from given conversations.
    ///
    /// N.B.: `all_mail` label is the only not removable label.
    async fn remove_all_labels<A>(
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = LabelId::all_mail()
            .into_inner()
            .counterpart::<Label, _>(interface)
            .await?
            .expect("AllMail should be set");
        for local_conversation_id in conversation_ids {
            interface
                .query_value::<_, LocalId>(
                    "DELETE FROM conversation_labels WHERE local_conversation_id = ? AND local_label_id != ?",
                    params![local_conversation_id, all_mail_id],
                )
                .await?;
        }
        Ok(())
    }

    /// Move conversations between two labels.
    ///
    /// # Parameters
    /// * `source_id`        - Local label id where the conversations currently are.
    /// * `destination_id`   - Local label id where the conversations should be moved.
    /// * `conversation_ids` - The IDs of the conversations to move.
    /// * `interface`        - The tether to use for the database connection.
    ///
    /// This function returns a tuple containing the source and destination remote label ids,
    /// respectively.
    ///
    /// # Remarks
    ///
    /// This function can only be called with an active transaction.
    ///
    /// # Errors
    ///
    /// Returns errors if the operation failed.
    pub async fn move_conversations<A>(
        source_id: LocalId,
        destination_id: LocalId,
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let remote_source_id = Label::resolve_remote_label_id(source_id, interface).await?;
        let remote_destination_id =
            Label::resolve_remote_label_id(destination_id, interface).await?;

        // If moving to trash, mark conversations as read.
        if remote_destination_id == LabelId::trash() {
            Conversation::mark_read(conversation_ids.clone(), interface)
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations as read when moving to trash: {e}");
                    e
                })?
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if remote_destination_id == LabelId::trash() || remote_destination_id == LabelId::spam() {
            Conversation::remove_all_labels(conversation_ids.clone(), interface)
                .await
                .inspect_err(|e| error!("Failed to remove labels: {e}"))?;
        } else if remote_source_id == LabelId::trash() || remote_source_id == LabelId::spam() {
            // When moving out of Trash or Spam, add AlmostAllMail label
            let almost_all_mail =
                Label::resolve_local_label_id(LabelId::almost_all_mail(), interface).await?;
            Conversation::apply_label(almost_all_mail, conversation_ids.clone(), interface)
                .await
                .inspect_err(|e| {
                    error!(
                        "Failed to apply almost all mail label when moving out of spam/trash:{e}"
                    )
                })?;
        }

        let Some(source) = Label::load(source_id, interface).await? else {
            return Err(AppError::LabelNotFound(source_id));
        };
        if source.is_movable_folder() {
            Conversation::remove_label(source_id, conversation_ids.clone(), interface).await?
        }

        Conversation::apply_label(destination_id, conversation_ids.clone(), interface).await?;

        Ok(())
    }

    /// Get the available actions for conversations excluding move to current view
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if
    ///
    /// * the database request fail,
    /// * empty list of conversations is provided
    /// * conversation is not in the view
    ///
    pub async fn available_actions<A>(
        view: Label,
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<ConversationAvailableActions, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;

        let mut starred = true;
        let mut deleted = true;
        let mut unread = false;
        let mut reply_all = false;

        for conversation in conversations.iter() {
            if !conversation.is_starred() {
                starred = false;
            }
            if !conversation.deleted {
                deleted = false;
            }
            if conversation.num_unread > 0 {
                unread = true;
            }
            if conversation.recipients.value.len() > 1 {
                reply_all = true;
            }
            let is_conversation_in_view = conversation
                .labels
                .iter()
                .any(|label| label.local_label_id == view.local_id);

            if !is_conversation_in_view {
                return Err(AppError::ConversationDoesNotHaveLabel(
                    conversation.local_id.unwrap(),
                    view.name.clone(),
                ));
            }
        }

        let mut conversation_actions = vec![
            if starred {
                ConversationAction::Unstar
            } else {
                ConversationAction::Star
            },
            if unread {
                ConversationAction::MarkRead
            } else {
                ConversationAction::MarkUnread
            },
            // Statics
            ConversationAction::Pin,
            ConversationAction::LabelAs,
        ];

        if !deleted {
            conversation_actions.push(ConversationAction::Delete);
        }

        let all_system = Label::find_by_kind(LabelType::System, interface).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let move_actions = conversations
            .iter()
            .flat_map(|conversation| {
                MoveAction::vec(all_system_excluding_view.clone(), |label| {
                    conversation
                        .labels
                        .iter()
                        .map(|conv_label| conv_label.local_label_id)
                        .contains(&label.local_id)
                })
            })
            .collect_vec();
        let reply_actions = if reply_all {
            ReplyAction::all()
        } else {
            ReplyAction::single_address()
        };

        Ok(ConversationAvailableActions::builder()
            .move_actions(MoveAction::system(move_actions))
            .reply_actions(reply_actions)
            .conversation_actions(conversation_actions)
            .build())
    }

    /// Get the available `label as` actions for conversations
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn available_label_as_actions<A>(
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<LabelAsAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let all_label_as = Label::find_by_kind(LabelType::Label, interface).await?;
        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;
        let all_label_as_actions = conversations
            .iter()
            .flat_map(|conversation| {
                LabelAsAction::vec(all_label_as.iter(), |label| {
                    conversation
                        .custom_labels
                        .iter()
                        .map(|label| Some(label.local_id))
                        .contains(&label.local_id)
                })
            })
            .collect_vec();

        Ok(LabelAsAction::finalize(all_label_as_actions))
    }

    /// Get the available move actions for conversations
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn available_move_to_actions<A>(
        view: Label,
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<MoveAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let all_system = Label::find_by_kind(LabelType::System, interface).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let all_custom_folders = Label::find_by_kind(LabelType::Folder, interface).await?;
        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;

        conversations.iter().try_for_each(|conversation| {
            let is_conversation_in_view = conversation
                .labels
                .iter()
                .map(|conv_label| conv_label.local_label_id)
                .any(|local_id| local_id == view.local_id);

            if is_conversation_in_view {
                Ok(())
            } else {
                Err(AppError::ConversationDoesNotHaveLabel(
                    conversation.local_id.unwrap(),
                    view.name.clone(),
                ))
            }
        })?;

        let all_move_to_actions = conversations
            .iter()
            .flat_map(|conversation| {
                MoveAction::vec(
                    all_system_excluding_view
                        .clone()
                        .chain(all_custom_folders.iter()),
                    |label| {
                        conversation
                            .labels
                            .iter()
                            .map(|conv_label| conv_label.local_label_id)
                            .contains(&label.local_id)
                    },
                )
            })
            .collect_vec();

        MoveAction::finalize(all_move_to_actions, interface).await
    }

    /// Finds all the messages from this conversation
    pub async fn load_messages<A>(&self, interface: &A) -> Result<Vec<Message>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Message::find(
            "WHERE local_conversation_id == ? ORDER BY time ASC, display_order ASC",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await
    }

    /// Finds all the conversations that have expired and deletes them and all of its
    /// messages.
    pub async fn delete_expired<A>(interface: &A) -> Result<usize, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let ids = Self::find_local_ids(
            r"
        WHERE
          expiration_time < STRFTIME('%s', 'NOW')
          AND expiration_time != 0
        ",
            vec![],
            interface,
        )
        .await?;

        let len = ids.len();

        if len != 0 {
            let label_id = SystemLabel::AllMail
                .local_id(interface)
                .await?
                .ok_or_else(|| StashError::IdNotSet)?;
            Self::mark_deleted(label_id, ids, interface).await?;
        }

        Ok(len)
    }

    #[cfg(test)]
    // TODO: Figure out how we want to do this in the future.
    ///
    /// Intended for testing only
    /// (local_attachment_id, local_message_id)
    /// Sets a conversation to be deleted in `expire_in` ms
    pub async fn set_expiration_time_in<A>(
        id: LocalId,
        expire_in: i64,
        db: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let affected = db
            .execute(
                r"
            UPDATE
                conversations
            SET
                expiration_time = (STRFTIME('%s', 'NOW') + ?)
            WHERE
                local_id = ?
            ",
                params![expire_in, id],
            )
            .await?;
        if affected != 1 {
            Err(StashError::Custom(String::from("No conversation found")))
        } else {
            Ok(())
        }
    }

    async fn check_has_label_and_is_unread<A>(
        local_label_id: LocalId,
        local_conversation_id: LocalId,
        interface: &A,
    ) -> Result<(bool, bool), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(label) = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND local_label_id=?",
            params![local_conversation_id, local_label_id],
            interface,
        )
        .await?
        {
            Ok((true, label.context_num_unread != 0))
        } else {
            Ok((false, false))
        }
    }

    /// Shared implementation to apply a label for messages and conversation.
    ///
    /// # Params
    ///
    /// * `local_label_id`         - Local label id of the [`Label`].
    /// * `local_conversation_id`  - Local conversation id to which the label
    ///                              should be applied.
    /// * `local_message_ids`      - Local ids of the messages which belong to
    ///                              `local_conversation_id` where the label
    ///                              should be applied.
    async fn label_impl<A>(
        local_label_id: LocalId,
        local_conversation_id: LocalId,
        local_message_ids: &[LocalId],
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_message_ids.is_empty() {
            return Ok(());
        }

        let (has_label, is_unread) = Conversation::check_has_label_and_is_unread(
            local_label_id,
            local_conversation_id,
            interface,
        )
        .await?;

        let stats = ConversationMessageLabelStats::with(
            local_conversation_id,
            local_label_id,
            local_message_ids,
            interface,
        )
        .await?;

        // Update conversation labels.
        let mut conversation_label = if let Some(mut label) = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND local_label_id=?",
            params![local_conversation_id, local_label_id],
            interface,
        )
        .await?
        {
            label.context_time = label.context_time.max(stats.time);
            label.context_snooze_time = label.context_snooze_time.max(stats.snooze_time);
            label.context_expiration_time =
                label.context_expiration_time.max(stats.expiration_time);
            label.context_size += stats.size;
            label.context_num_unread += stats.unread;
            label.context_num_attachments += stats.num_attachments as u64;
            label.context_num_messages += stats.count;
            label
        } else {
            let remote_label_id =
                if let Some(label) = Label::find_by_id(local_label_id, interface).await? {
                    label.remote_id
                } else {
                    None
                };
            ConversationLabel {
                local_id: None,
                local_conversation_id: Some(local_conversation_id),
                local_label_id: Some(local_label_id),
                remote_label_id,
                context_expiration_time: stats.expiration_time,
                context_num_attachments: stats.num_attachments as u64,
                context_num_messages: stats.count,
                context_num_unread: stats.unread,
                context_size: stats.size,
                context_snooze_time: stats.snooze_time,
                context_time: stats.time,
                deleted: false,
                row_id: None,
                stash: None,
            }
        };

        conversation_label.save_using(interface).await?;

        // Update message label counts.
        let Some(mut label) = Label::find_by_id(local_label_id, interface).await? else {
            error!("Could not find label");
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        label.unread_msg += stats.unread;
        label.total_msg += stats.count;

        let should_increment_count = !has_label;
        let should_increment_unread = !is_unread && stats.unread != 0;

        label.total_conv += should_increment_count as u64;
        label.unread_conv += should_increment_unread as u64;

        label.save_using(interface).await?;

        Ok(())
    }

    /// Sync the conversation message for `local_conversation_id` from the server.
    ///
    /// The messages are only synced once if `has_messages` is not set to true.
    /// Future updates are expected to happen via the event loop.
    ///
    /// If `has_messages` is true, nothing is done.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed or if the server request failed.
    pub async fn sync_conversation_messages<A, PM>(
        local_conversation_id: LocalId,
        interface: &A,
        api: &PM,
    ) -> Result<(), AppError>
    where
        PM: ProtonMail,
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(conversation) = Self::find_by_id(local_conversation_id, interface).await? else {
            return Err(AppError::ConversationNotFound(local_conversation_id));
        };

        if !conversation.has_messages {
            let Some(rid) = conversation.remote_id else {
                return Err(AppError::LabelDoesNotHaveRemoteId(local_conversation_id));
            };
            debug!("Syncing conversation messages");
            let conversation_response = api.get_conversation(rid.into()).await.map_err(|e| {
                error!("failed to download conversation messages: {e}");
                AppError::from(e)
            })?;

            let tx = interface.transaction().await?;

            let message_metadata: Vec<ApiMessageMetadata> = conversation_response
                .messages
                .into_iter()
                .map(Into::into)
                .collect();
            let mut new_conversation: Conversation = conversation_response.conversation.into();

            Message::create_or_update_messages_from_metadata(message_metadata, &tx)
                .await
                .map_err(|e| {
                    error!("Failed to write message metadata: {e}");
                    e
                })?;

            new_conversation.local_id = conversation.local_id;
            new_conversation.row_id = conversation.row_id;
            new_conversation.has_messages = true;

            new_conversation.save_using(&tx).await.map_err(|e| {
                error!("Failed to write conversation: {e}");
                e
            })?;

            tx.commit().await?;
        } else {
            debug!("Conversation messages already synced")
        }

        Ok(())
    }

    /// Retrieve all the conversation which are in a given label.
    ///
    /// # Params
    ///
    /// * `local_label_id` - Label where to search in
    /// * `interface`      - Connection to the database
    /// * `queue`          - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn in_label<A>(
        local_label_id: LocalId,
        interface: &A,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Conversation::find(
            formatdoc!(
                "
                JOIN conversation_labels
                    ON conversations.local_id = conversation_labels.local_conversation_id
                WHERE
                    conversation_labels.local_label_id = ?
                AND
                    conversation_labels.deleted = 0
                ORDER BY
                    conversation_labels.context_time DESC,
                    conversations.display_order DESC
                "
            ),
            params![local_label_id],
            interface,
            queue,
        )
        .await
    }

    /// Create a paginator for conversations in a given label.
    ///
    /// # Params
    ///
    /// * `context`        - Active user context.
    /// * `local_label_id` - Label where to paginate in.
    /// * `page_count`     - Number of elements per page.
    /// * `queue`          - Optional subscriber for changes.
    /// * `filter`         - Filter options for pagination.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn paginate_in_label(
        context: &MailUserContext,
        local_label_id: LocalId,
        page_count: u32,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
        filter: PaginatorFilter,
    ) -> Result<PaginatorCompat<Self, ConversationDataSource>, AppError> {
        let remote_source =
            ConversationDataSource::new(context, local_label_id, filter.clone()).await?;

        let mut query = formatdoc!(
            "
            JOIN conversation_labels
                ON conversations.local_id = conversation_labels.local_conversation_id
            WHERE
                conversation_labels.local_label_id = ?
            AND
                conversation_labels.deleted = 0
            "
        );

        let params = vec![Param::Integer(
            i64::try_from(local_label_id.as_u64()).map_err(|err| {
                StashError::ExecutionError(SqliteError::ToSqlConversionFailure(Box::new(err)))
            })?,
        )];

        if let Some(unread) = filter.unread {
            query += &format!(
                "AND conversation_labels.context_num_unread {} 0 ",
                if unread { ">" } else { "=" }
            );
        }

        query += "ORDER BY
            conversation_labels.context_time DESC,
            conversations.display_order DESC
        ";

        Ok(PaginatorCompat::new(
            Paginator::new(
                query,
                params,
                context.user_stash(),
                NonZeroU32::new(page_count)
                    .ok_or(StashError::Custom("Invalid Page Count value".to_owned()))?,
                remote_source,
                queue,
            )
            .await?,
        ))
    }
    /// This fn should be called for conversation endpoints.
    /// Repeatedly calls `endpoint` in batches of 1 in parallel.
    async fn split_request<F, Fut>(
        ids: impl IntoIterator<Item = RemoteId>,
        endpoint: F,
    ) -> Result<Vec<OperationResult>, ApiServiceError>
    where
        F: Fn(Vec<ApiRemoteId>) -> Fut,
        Fut: Future<Output = Result<Vec<OperationResult>, ApiServiceError>>,
    {
        split_request(ids, 1, endpoint).await
    }

    /// Get the possible next display order.
    ///
    /// Finds the maximum display order value in all conversations and adds 1
    /// to the existing value.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    ///
    pub async fn next_display_order<A>(interface: &A) -> Result<u64, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(interface
            .query_value::<_, u64>(
                format!(
                    "SELECT IFNULL(MAX(display_order),0) AS value FROM {}",
                    Self::table_name()
                ),
                vec![],
            )
            .await?
            .saturating_add(1))
    }

    /// Only get Disposition::Attachment attachments
    pub fn get_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Attachment))
            .cloned()
            .collect()
    }

    /// Only get Disposition::Inline attachments
    #[allow(dead_code)] // Will get used later on
    fn get_inline_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Inline))
            .cloned()
            .collect()
    }
}

impl From<ApiConversation> for Conversation {
    fn from(value: ApiConversation) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            attachment_info: MessageAttachmentInfos {
                value: value
                    .attachment_info
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect(),
            },
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(AttachmentMetadata::from)
                .collect(),
            deleted: false,
            display_snooze_reminder: value.display_snooze_reminder,
            expiration_time: value.expiration_time,
            exclusive_location: None,
            labels: value.labels.into_iter().map(|v| v.into()).collect(),
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            display_order: value.order,
            recipients: MessageAddresses {
                value: value.recipients.into_iter().map(|v| v.into()).collect(),
            },
            senders: MessageAddresses {
                value: value.senders.into_iter().map(|v| v.into()).collect(),
            },
            custom_labels: vec![],
            size: value.size,
            subject: value.subject,
            row_id: None,
            stash: None,
            is_known: true,
            has_messages: false,
        }
    }
}

/// Contextual label metadata associated with a Conversation.
///
/// When a conversation is opened in the context of label, the
/// [`ConversationLabel`] information is superimposed over the [`Conversation`]
/// for that context.
///
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("conversation_labels")]
pub struct ConversationLabel {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_conversation_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_label_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_label_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub context_expiration_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_attachments: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_messages: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_unread: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_size: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_snooze_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_time: u64,

    #[DbField]
    pub deleted: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl ConversationLabel {
    /// Get all local label ids for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn labels_ids_for_conversation<A>(
        conversation_id: LocalId,
        interface: &A,
    ) -> Result<Vec<LocalId>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let query = format!(
            "SELECT local_label_id as value FROM {} WHERE local_conversation_id = ?",
            Self::table_name()
        );

        interface
            .query_values::<_, LocalId>(&query, params![conversation_id])
            .await
    }

    /// Get all local label with given IDs.
    ///
    /// # Parameters
    ///
    /// * `label_ids` - List of ids we want to find the corresponding `ConversationLabel`.
    /// * `interface` - The database interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn find_by_ids<A>(
        label_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        ConversationLabel::find(
            format!("WHERE local_id IN ({})", label_ids.into_iter().join(", ")),
            vec![],
            interface,
            None,
        )
        .await
    }

    /// Save or update a Conversation Label.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// The current stash database does not allow us to resolve conflicts on
    /// other unique keys so we have to do this ourselves.
    /// If [`Model::save()`] is used directly it will bypass this check.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update a Conversation Label.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// The current stash database does not allow us to resolve conflicts on
    /// other unique keys so we have to do this ourselves.
    /// If [`Model::save_using()`] is used directly it will bypass this check.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(local_conversation_id) = self.local_conversation_id else {
            return Err(StashError::Custom(
                "Missing local conversation id".to_owned(),
            ));
        };

        let Some(remote_label_id) = self.remote_label_id.clone() else {
            return Err(StashError::Custom("Missing remote label id".to_owned()));
        };

        let Some(local_label) =
            Label::find_by_id(RemoteId::from(remote_label_id.clone()), interface).await?
        else {
            return Err(StashError::Custom(format!(
                "Can't find label with the remote id {remote_label_id}"
            )));
        };

        self.local_label_id = local_label.local_id;

        if let Some(label) = ConversationLabel::find_first(
            "WHERE local_label_id=? AND local_conversation_id=?",
            params![
                local_label.local_id.expect("Should be set"),
                local_conversation_id
            ],
            interface,
        )
        .await?
        {
            self.local_id = label.local_id;
            self.row_id = label.row_id;
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as deleted.
    ///
    /// ## Parameters
    ///
    /// * `stats` - The stats of the message that was deleted.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    async fn mark_delete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.context_num_messages = self.context_num_messages.saturating_sub(stats.count);
            self.context_num_unread = self.context_num_unread.saturating_sub(stats.unread_count);
            self.context_num_attachments = self
                .context_num_attachments
                .saturating_sub(stats.attachment_count);
            self.context_size = self.context_size.saturating_sub(stats.size);
            self.save_using(interface).await?;
        }

        Ok(())
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as undeleted.
    ///
    /// ## Parameters
    ///
    /// * `stats` - The stats of the message that was undeleted.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    async fn mark_undelete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.context_num_messages += stats.count;
            self.context_num_unread += stats.unread_count;
            self.context_num_attachments += stats.attachment_count;
            self.context_size += stats.size;
            self.save_using(interface).await?;
        }

        Ok(())
    }
}

impl From<ApiConversationLabel> for ConversationLabel {
    fn from(value: ApiConversationLabel) -> Self {
        Self {
            local_id: None,
            local_conversation_id: None,
            local_label_id: None,
            remote_label_id: Some(value.id.into()),
            context_expiration_time: value.context_expiration_time,
            context_num_attachments: value.context_num_attachments,
            context_num_messages: value.context_num_messages,
            context_num_unread: value.context_num_unread,
            context_size: value.context_size,
            context_snooze_time: value.context_snooze_time,
            context_time: value.context_time,
            deleted: false,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[ModelActions(on_load, on_save)]
#[TableName("labels")]
pub struct Label {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_parent_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_parent_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub color: LabelColor,

    /// TODO: Document this field.
    #[DbField]
    pub display: bool,

    /// TODO: Document this field.
    #[DbField]
    pub expanded: bool,

    /// TODO: Document this field.
    #[DbField]
    pub initialized_conv: bool,

    /// TODO: Document this field.
    #[DbField]
    pub initialized_msg: bool,

    /// TODO: Document this field.
    #[DbField]
    pub label_type: LabelType,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    /// TODO: Document this field.
    #[DbField]
    pub notify: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub path: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub sticky: bool,

    /// TODO: Document this field.
    #[DbField]
    pub total_conv: u64,

    /// TODO: Document this field.
    #[DbField]
    pub total_msg: u64,

    /// TODO: Document this field.
    #[DbField]
    pub unread_conv: u64,

    /// TODO: Document this field.
    #[DbField]
    pub unread_msg: u64,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Label {
    /// Save or update a Label.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update a Label.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(label) =
                Label::find_first("WHERE remote_id=?", params![remote_id], interface).await?
            {
                self.local_parent_id = label.local_parent_id;
                self.local_id = label.local_id;
                self.row_id = label.row_id;
                self.stash = label.stash;
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `name`       - TODO: Document this parameter.
    /// * `color`      - TODO: Document this parameter.
    /// * `label_type` - TODO: Document this parameter.
    /// * `parent_id`  - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn create<PM: ProtonMail>(
        name: String,
        color: String,
        label_type: LabelType,
        parent_id: Option<LabelId>,
        api: &PM,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .post_labels(PostLabelsRequest {
                parent_id: parent_id.map(|id| id.into()),
                color,
                label_type: label_type.into(),
                name,
            })
            .await?
            .label
            .into())
    }

    pub async fn create_or_update_conversation_counts<A>(
        counts: Vec<ConversationCount>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for count in counts {
            interface
                .execute(
                    formatdoc!(
                        r"
                    UPDATE
                        labels
                    SET
                        total_conv = ?,
                        unread_conv = ?
                    WHERE
                        remote_id = ?
                    "
                    ),
                    params![count.total, count.unread, count.label_id],
                )
                .await?;
        }
        Ok(())
    }

    pub async fn create_or_update_message_counts<A>(
        counts: Vec<MessageCount>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for count in counts {
            interface
                .execute(
                    formatdoc!(
                        r"
                    UPDATE
                        labels
                    SET
                        total_msg = ?,
                        unread_msg = ?
                    WHERE
                        remote_id = ?
                    "
                    ),
                    params![count.total, count.unread, count.label_id],
                )
                .await?;
        }
        Ok(())
    }

    /// TODO: Document this function.
    pub fn is_applicable_label(&self) -> bool {
        self.label_type == LabelType::Label || self.is_starred()
    }

    /// Checks if label is a System label - starred.
    pub fn is_starred(&self) -> bool {
        self.remote_id
            .as_ref()
            .map_or(false, |rid| *rid == LabelId::starred())
    }

    /// TODO: Document this function.
    pub fn is_movable_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self.remote_id.as_ref().map_or(false, |rid| {
                LabelId::movable_sys_folder_list().contains(rid)
            })
    }

    /// Fetches all labels from the API and stores them in the database.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_labels<PM: ProtonMail, A>(api: &PM, interface: &A) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_requests =
            futures::future::join_all(ALL_LABEL_TYPES.into_iter().map(|category| {
                debug!("Fetching labels ({:?})", category);
                api.get_labels(category.into())
            }))
            .await;

        debug!("Storing labels into database");
        let tx = interface.transaction().await?;
        for labels in label_requests {
            match labels {
                Err(e) => {
                    error!("Failed to fetch labels: {e}");
                    tx.commit().await?;
                    return Err(AppError::from(e));
                }
                Ok(labels) => {
                    for mut label in labels.labels.into_iter().map_into::<Self>() {
                        label.save_using(&tx).await?;
                    }
                }
            }
        }
        tx.commit().await?;

        Ok(())
    }

    /// Fetches the given labels from the API and stores them in the database.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_labels_by_ids<PM: ProtonMail, A>(
        api: &PM,
        interface: &A,
        ids: Vec<ApiRemoteId>,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let labels = api
            .get_labels_by_ids(ids)
            .await?
            .labels
            .into_iter()
            .map_into::<Self>();

        debug!("Storing labels into database");
        let tx = interface.transaction().await?;
        for mut label in labels {
            Self::save_using(&mut label, &tx).await?;
        }
        tx.commit().await?;

        Ok(())
    }

    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        if self.remote_parent_id.is_some() && self.local_parent_id.is_none() {
            self.local_parent_id = self
                .remote_parent_id
                .clone()
                .expect("Should be set")
                .counterpart::<Self, _>(interface)
                .await?;
        }
        // TODO: https://jira.protontech.ch/browse/ET-1169 ensure that local_remote_id are resolve for Label
        Ok(())
    }

    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        let parent_id_option = self.remote_parent_id.clone();
        self.local_parent_id = match parent_id_option {
            Some(parent_id) => {
                let res = parent_id.counterpart::<Self, _>(interface).await?;
                if res.is_none() {
                    // TODO: handle this error
                    error!(
                        "A Label({:?}) remote_parent don't have corresponding local_id",
                        self.remote_id
                    );
                }
                res
            }
            None => None,
        };
        interface
            .execute(
                format!(
                    "UPDATE {} SET local_parent_id=? WHERE local_id=?",
                    Label::table_name()
                ),
                params![self.local_parent_id, self.local_id],
            )
            .await?;
        Ok(())
    }

    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `id`         - The ID of the label to update.
    /// * `name`       - TODO: Document this parameter.
    /// * `color`      - TODO: Document this parameter.
    /// * `label_type` - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn update<PM: ProtonMail>(
        id: LabelId,
        name: String,
        color: String,
        parent_id: Option<LabelId>,
        api: &PM,
    ) -> Result<Label, ApiServiceError> {
        Ok(api
            .put_label(
                id.into(),
                PutLabelRequest {
                    parent_id: parent_id.map(|id| id.into()),
                    color,
                    name,
                },
            )
            .await?
            .label
            .into())
    }

    /// Function to update the label's expanded state in remote.
    ///
    /// # Parameters
    ///
    /// * `id`         - The Remote ID of the label to update.
    /// * `expanded`   - The new expanded state.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn patch_expanded<PM: ProtonMail>(
        id: LabelId,
        expanded: bool,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        api.patch_label(
            id.into(),
            PatchLabelRequest {
                expanded: Some(expanded),
                ..Default::default()
            },
        )
        .await
        .map(|r| r.responses)
    }

    /// Return the preferred view mode for this label.
    ///
    /// If this function returns [`None`] we should use the [`ViewMode`] defined
    /// in the user's [`MailSettings`], otherwise the returned value should be
    /// used.
    ///
    pub async fn view_mode<A>(&self, interface: &A) -> Result<ViewMode, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.as_ref() {
            if *remote_id == LabelId::drafts()
                || *remote_id == LabelId::sent()
                || *remote_id == LabelId::all_drafts()
                || *remote_id == LabelId::all_sent()
                || *remote_id == LabelId::all_scheduled()
            {
                return Ok(ViewMode::Messages);
            }
        }
        Ok(MailSettings::load(MAIL_SETTINGS_ID.into(), interface)
            .await?
            .unwrap_or_default()
            .view_mode)
    }

    /// Get all labels with given kind
    ///
    /// # Parameters
    ///
    /// * `kind` - The kind of the label, eg. System, Folder etc.
    /// * `tx`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_by_kind<A>(kind: LabelType, interface: &A) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Label::find(
            "WHERE label_type = ? ORDER BY display_order ASC",
            params![kind],
            interface,
            None,
        )
        .await
    }

    /// Watch a label with the given `local_id` for changes.
    ///
    /// When a change occurs a message is produced in the returned receiver.
    ///
    /// Returns `None` if the label was not found.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn watch<A>(
        local_id: LocalId,
        interface: &A,
    ) -> Result<
        Option<(
            Self,
            flume::Receiver<ResultsetChange<Self, <Self as Model>::IdType>>,
        )>,
        AppError,
    >
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (sender, receiver) = flume::unbounded();
        let mut labels = Label::find(
            "WHERE local_id=?",
            params![local_id],
            interface,
            Some(sender),
        )
        .await?;
        if labels.is_empty() {
            return Ok(None);
        }

        Ok(Some((labels.swap_remove(0), receiver)))
    }

    /// Resolve the remote id for a label with `local_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_remote_label_id<A>(
        local_id: LocalId,
        interface: &A,
    ) -> Result<LabelId, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(label_id) = local_id.counterpart::<Label, _>(interface).await? else {
            return Err(AppError::LabelNotFound(local_id));
        };

        Ok(label_id.into())
    }

    /// Resolve the local id for a label with `label_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the resolution failed.
    pub async fn resolve_local_label_id<A>(
        label_id: LabelId,
        interface: &A,
    ) -> Result<LocalId, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(label_id) = label_id.counterpart::<Label, _>(interface).await? else {
            return Err(AppError::RemoteLabelDoesNotExist(label_id));
        };
        Ok(label_id)
    }
}

impl From<ApiLabel> for Label {
    fn from(value: ApiLabel) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            local_parent_id: None,
            remote_parent_id: value.parent_id.map(|id| id.into()),
            color: value.color.into(),
            display_order: value.order,
            display: value.display,
            expanded: value.expanded,
            initialized_conv: false,
            initialized_msg: false,
            label_type: value.label_type.into(),
            name: value.name,
            notify: value.notify,
            path: value.path,
            sticky: value.sticky,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: None,
        }
    }
}

impl Default for Label {
    fn default() -> Self {
        Self {
            label_type: LabelType::Label,
            local_id: Default::default(),
            remote_id: Default::default(),
            local_parent_id: Default::default(),
            remote_parent_id: Default::default(),
            color: Default::default(),
            display: Default::default(),
            expanded: Default::default(),
            initialized_conv: Default::default(),
            initialized_msg: Default::default(),
            name: Default::default(),
            notify: Default::default(),
            display_order: Default::default(),
            path: Default::default(),
            sticky: Default::default(),
            total_conv: Default::default(),
            total_msg: Default::default(),
            unread_conv: Default::default(),
            unread_msg: Default::default(),
            row_id: Default::default(),
            stash: Default::default(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq, SmartDefault)]
#[allow(clippy::struct_excessive_bools)]
#[TableName("mail_settings")]
pub struct MailSettings {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub almost_all_mail: AlmostAllMail,

    /// TODO: Document this field.
    #[DbField]
    pub attach_public_key: bool,

    /// TODO: Document this field.
    #[DbField]
    pub auto_delete_spam_and_trash_days: Option<u32>,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub auto_save_contacts: bool,

    /// TODO: Document this field.
    #[DbField]
    pub block_sender_confirmation: Option<bool>,

    /// TODO: Document this field.
    #[DbField]
    pub composer_mode: ComposerMode,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub confirm_link: bool,

    /// TODO: Document this field.
    #[DbField]
    #[default = 10]
    pub delay_send_seconds: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: String,

    /// TODO: Document this field.
    #[DbField]
    pub draft_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub enable_folder_color: bool,

    /// TODO: Document this field.
    #[DbField]
    pub font_face: Option<String>,

    /// This enables or disables remote content in the HTML.
    #[DbField]
    pub hide_remote_images: bool,

    /// This enables or disables embedded content (`Disposition::Inline`) in the HTML.
    #[DbField]
    pub hide_embedded_images: bool,

    /// TODO: Document this field.
    #[DbField]
    pub hide_sender_images: bool,

    /// TODO: Document this field.
    #[DbField]
    pub image_proxy: u32,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub inherit_parent_folder_color: bool,

    /// TODO: Document this field.
    #[DbField]
    pub message_buttons: MessageButtons,

    /// TODO: Document this field.
    #[DbField]
    pub mobile_settings: Option<MobileSettings>,

    /// TODO: Document this field.
    #[DbField]
    pub next_message_on_move: Option<NextMessageOnMove>,

    /// TODO: Document this field.
    #[DbField]
    pub num_message_per_page: u32,

    /// TODO: Document this field.
    #[DbField]
    pub pgp_scheme: PgpScheme,

    /// TODO: Document this field.
    #[DbField]
    pub pm_signature: PmSignature,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub pm_signature_referral_link: bool,

    /// TODO: Document this field.
    #[DbField]
    pub prompt_pin: bool,

    /// TODO: Document this field.
    #[DbField]
    pub receive_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub right_to_left: ComposerDirection,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub shortcuts: bool,

    /// TODO: Document this field.
    #[DbField]
    pub show_images: ShowImages,

    /// TODO: Document this field.
    #[DbField]
    pub show_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub show_moved: ShowMoved,

    /// TODO: Document this field.
    #[DbField]
    pub sign: bool,

    /// TODO: Document this field.
    #[DbField]
    pub signature: String,

    /// TODO: Document this field.
    #[DbField]
    pub spam_action: Option<SpamAction>,

    /// TODO: Document this field.
    #[DbField]
    pub sticky_labels: bool,

    /// TODO: Document this field.
    #[DbField]
    pub submission_access: bool,

    /// TODO: Document this field.
    #[DbField]
    pub swipe_left: SwipeAction,

    /// TODO: Document this field.
    #[DbField]
    pub swipe_right: SwipeAction,

    /// TODO: Document this field.
    #[DbField]
    pub theme: String,

    /// TODO: Document this field.
    #[DbField]
    pub view_layout: ViewLayout,

    /// TODO: Document this field.
    #[DbField]
    pub view_mode: ViewMode,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl MailSettings {
    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_mail_settings<PM: ProtonMail>(
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let mut settings = MailSettings::from(api.get_settings().await.map(|r| r.mail_settings)?);
        debug!("Storing labels into database");
        settings.set_stash(stash);

        let tx = stash.transaction().await?;
        settings.save_using(&tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Get the mail settings from database
    pub async fn get<A>(interface: &A) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Self::load(MAIL_SETTINGS_ID.into(), interface).await
    }

    /// Get the mail settings from database, fallback on default
    pub async fn get_or_default<A>(interface: &A) -> Self
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Self::get(interface)
            .await
            .unwrap_or_default()
            .unwrap_or_default()
    }
}

impl From<ApiMailSettings> for MailSettings {
    fn from(value: ApiMailSettings) -> Self {
        Self {
            local_id: None,
            almost_all_mail: value.almost_all_mail.into(),
            attach_public_key: value.attach_public_key,
            auto_delete_spam_and_trash_days: value.auto_delete_spam_and_trash_days,
            auto_save_contacts: value.auto_save_contacts,
            block_sender_confirmation: value.block_sender_confirmation,
            composer_mode: value.composer_mode.into(),
            confirm_link: value.confirm_link,
            delay_send_seconds: value.delay_send_seconds,
            display_name: value.display_name,
            draft_mime_type: value.draft_mime_type.into(),
            enable_folder_color: value.enable_folder_color,
            font_face: value.font_face,
            hide_remote_images: value.hide_remote_images,
            hide_embedded_images: value.hide_embedded_images,
            hide_sender_images: value.hide_sender_images,
            image_proxy: value.image_proxy,
            inherit_parent_folder_color: value.inherit_parent_folder_color,
            message_buttons: value.message_buttons.into(),
            mobile_settings: value.mobile_settings.map(Into::into),
            next_message_on_move: value.next_message_on_move.map(Into::into),
            num_message_per_page: value.num_message_per_page,
            pgp_scheme: value.pgp_scheme.into(),
            pm_signature: value.pm_signature.into(),
            pm_signature_referral_link: value.pm_signature_referral_link,
            prompt_pin: value.prompt_pin,
            receive_mime_type: value.receive_mime_type.into(),
            right_to_left: value.right_to_left.into(),
            shortcuts: value.shortcuts,
            show_images: value.show_images.into(),
            show_mime_type: value.show_mime_type.into(),
            show_moved: value.show_moved.into(),
            sign: value.sign,
            signature: value.signature,
            spam_action: value.spam_action.map(Into::into),
            sticky_labels: value.sticky_labels,
            submission_access: value.submission_access,
            swipe_left: value.swipe_left.into(),
            swipe_right: value.swipe_right.into(),
            theme: value.theme,
            view_layout: value.view_layout.into(),
            view_mode: value.view_mode.into(),
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("messages")]
#[ModelActions(on_load, on_save)]
pub struct Message {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_conversation_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_conversation_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_address_id: LocalId,

    /// TODO: Document this field.
    #[DbField]
    pub remote_address_id: RemoteId,

    /// TODO: Document this field.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[DbField]
    pub cc_list: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub bcc_list: MessageAddresses,

    /// Whether or not this message has been soft deleted. This means that this message
    /// should no longer be displayed.
    #[DbField]
    pub deleted: bool,

    /// Exclusive location of the [`Message`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

    /// The unix timestamp at which this message is set to expire at.
    /// 0 means that it will not expire.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub external_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub flags: MessageFlags,

    /// TODO: Document this field.
    #[DbField]
    pub is_forwarded: bool,

    /// TODO: Document this field.
    #[DbField]
    pub is_replied: bool,

    /// TODO: Document this field.
    #[DbField]
    pub is_replied_all: bool,

    /// TODO: Document this field.
    pub label_ids: Vec<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub num_attachments: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u64,

    /// TODO: Document this field.
    #[DbField]
    pub reply_tos: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub sender: MessageAddress,

    /// TODO: Document this field.
    #[DbField]
    pub size: u64,

    /// TODO: Document this field.
    #[DbField]
    pub snooze_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub subject: String,

    /// TODO: Document this field.
    #[DbField]
    pub time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub to_list: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub unread: bool,

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// True when message body is in cache.
    #[DbField]
    pub cached: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Message {
    /// Save a message to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that local ids are resolved before they can be written
    /// to the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a message to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that local ids are resolved before they can be written
    /// to the database.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
                self.stash = existing.stash;
            }
        }

        if self.local_conversation_id.is_none() {
            if let Some(remote_conversation_id) = self.remote_conversation_id.clone() {
                if let Some(conversation) =
                    Conversation::find_by_id(remote_conversation_id.clone(), interface).await?
                {
                    self.local_conversation_id = conversation.local_id;
                } else {
                    // Create an unknown entry.
                    let mut conversation = Conversation::unknown(remote_conversation_id);
                    conversation.save_using(interface).await?;
                    self.local_conversation_id = conversation.local_id;
                }
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Given a vec of message metadatas tries to create them in the database
    ///
    /// # Parameters
    ///
    /// * `metadata`  - The message metadata returned from the API
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for accessing the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn create_or_update_messages_from_metadata_vec<A>(
        metadata: Vec<ApiMessageMetadata>,
        interface: &A,
    ) -> Result<Vec<Message>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut ids = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            let mut message = Message::from_api_metadata(metadata, interface).await?;
            Self::save_using(&mut message, interface).await?;
            ids.push(message);
        }
        Ok(ids)
    }

    /// Given a message metadata tries to create it in the database
    ///
    /// # Parameters
    ///
    /// * `metadata`  - The message metadata returned from the API
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for accessing the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn create_or_update_messages_from_metadata<A>(
        metadata: Vec<ApiMessageMetadata>,
        interface: &A,
    ) -> Result<Vec<LocalId>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(
            Self::create_or_update_messages_from_metadata_vec(metadata, interface)
                .await?
                .into_iter()
                .filter_map(|x| x.local_id)
                .collect(),
        )
    }

    /// Delete multiple messages.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the messages to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn delete_multiple_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_messages_delete(ids, Some(label_id.into()))
                    .await
                    .map(|r| r.responses)
            }
        };
        Message::split_request(ids, request).await
    }

    /// Mark messages as deleted.
    ///
    /// This is soft delete of messages. It will assign deleted flag to true,
    /// Adjust labels, conversations and conversation labels stats.
    /// Morover if all messages within a conversation were deleted, the conversation
    /// will be deleted as well.
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_deleted<A>(ids: Vec<LocalId>, interface: &A) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (query, params) = find_in_query!("WHERE deleted = 0 AND local_id IN ({})", ids);
        let messages = Message::find(query, params, interface, None).await?;
        let mut messages_by_conversation = HashMap::new();

        for mut message in messages {
            message.deleted = true;
            message.save_using(interface).await?;
            messages_by_conversation
                .entry(message.local_conversation_id)
                .or_insert_with(Vec::new)
                .push(message);
        }

        for (conversation_id, messages) in messages_by_conversation {
            let all_stats =
                Message::update_message_counters_after_soft_delete(messages, interface).await?;
            let conversation = Conversation::find_first(
                "WHERE local_id=? AND deleted=0 AND is_known=1",
                params![conversation_id],
                interface,
            )
            .await?;

            if let Some(mut conversation) = conversation {
                let label_ids = all_stats.keys().copied().collect::<Vec<_>>();
                let (query, mut params) = find_in_query!(
                    "WHERE local_conversation_id=? AND deleted=0 AND local_label_id IN ({})",
                    label_ids
                );
                params.insert(
                    0,
                    Box::new(conversation.local_id.unwrap()) as Box<dyn ToSql + Send>,
                );

                let conv_labels = ConversationLabel::find(query, params, interface, None).await?;
                let all_mail_stats = SystemLabel::AllMail
                    .local_id(interface)
                    .await?
                    .and_then(|id| all_stats.get(&id));

                conversation
                    .mark_delete_update_stats(all_mail_stats, interface)
                    .await?;

                for mut conv_label in conv_labels {
                    let label_id = &conv_label.local_label_id.unwrap();
                    conv_label
                        .mark_delete_update_stats(all_stats.get(label_id), interface)
                        .await?;
                }

                if conversation.deleted {
                    for (label_id, stats) in all_stats.iter() {
                        conversation
                            .remove_conversation_from_label(*label_id, Some(stats), interface)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Mark messages as undeleted.
    ///
    /// This is soft undelete of messages. It will assign deleted flag to false,
    /// Adjust labels, conversations and conversation labels stats.
    /// Morover if conversation was deleted it will be restored.
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the messages to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_undeleted<A>(ids: Vec<LocalId>, interface: &A) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (query, params) = find_in_query!("WHERE deleted = 1 AND local_id IN ({})", ids);
        let messages = Message::find(query, params, interface, None).await?;
        let mut messages_by_conversation = HashMap::new();

        for mut message in messages {
            message.deleted = false;
            message.save_using(interface).await?;
            messages_by_conversation
                .entry(message.local_conversation_id)
                .or_insert_with(Vec::new)
                .push(message);
        }

        for (conversation_id, messages) in messages_by_conversation {
            let all_stats =
                Message::update_message_counters_after_soft_undelete(messages, interface).await?;
            let conversation =
                Conversation::find_first("WHERE local_id=?", params![conversation_id], interface)
                    .await?;

            if let Some(mut conversation) = conversation {
                if conversation.deleted {
                    for (label_id, stats) in all_stats.iter() {
                        conversation
                            .add_conversation_to_label(*label_id, Some(stats), interface)
                            .await?;
                    }
                }

                let label_ids = all_stats.keys().copied().collect::<Vec<_>>();
                let (query, mut params) = find_in_query!(
                    "WHERE local_conversation_id=? AND deleted=0 AND local_label_id IN ({})",
                    label_ids
                );
                params.insert(
                    0,
                    Box::new(conversation.local_id.unwrap()) as Box<dyn ToSql + Send>,
                );

                let conv_labels = ConversationLabel::find(query, params, interface, None).await?;
                let all_mail_stats = SystemLabel::AllMail
                    .local_id(interface)
                    .await?
                    .and_then(|id| all_stats.get(&id));

                conversation
                    .mark_undelete_update_stats(all_mail_stats, interface)
                    .await?;

                for mut conv_label in conv_labels {
                    let label_id = &conv_label.local_label_id.unwrap();

                    conv_label
                        .mark_undelete_update_stats(all_stats.get(label_id), interface)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Get the message counts.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_counts<PM: ProtonMail>(
        api: &PM,
    ) -> Result<Vec<MessageCount>, ApiServiceError> {
        api.get_messages_count()
            .await
            .map(|r| r.counts.into_iter().map(|c| c.into()).collect())
    }

    /// Get message metadata.
    ///
    /// # Parameters
    ///
    /// * `filter` - The filter to use.
    /// * `api`    - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_metadata<PM: ProtonMail>(
        filter: GetMessagesOptions,
        api: &PM,
    ) -> Result<GetMessagesResponse, ApiServiceError> {
        api.get_messages(filter).await
    }

    /// Get all labels for the message.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn all_message_labels<A>(&self, interface: &A) -> Result<Vec<Label>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let labels = Label::find(
            r#"
            WHERE local_id IN (
                SELECT local_label_id FROM message_labels WHERE local_message_id = ?
            ) ORDER BY display_order ASC
            "#,
            params![self.local_id],
            interface,
            None,
        )
        .await?;

        Ok(labels)
    }

    /// Extends [`Model::load()`] to pre-load child records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        self.attachments_metadata =
            Attachment::load_message_attachment_metadata(self.local_id.unwrap(), interface).await?;

        let labels = self.all_message_labels(interface).await?;

        self.exclusive_location = ExclusiveLocation::from_labels(&labels);
        self.label_ids = labels
            .iter()
            .map(|l| l.remote_id.clone().unwrap())
            .collect();

        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        // TODO: The message body might need to be loaded in here, but it's not
        // TODO: totally clear how best to do that seeing as the cache feature
        // TODO: requires some additional parameters such as the path. So this can
        // TODO: currently be done as a subsequent manual step.

        Ok(())
    }

    /// Extends [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this message.
        if !self.label_ids.is_empty() {
            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id = ?
                    AND local_label_id NOT IN (
                        SELECT local_id FROM labels WHERE remote_id IN ({})
                    )
                ",
                        vec!["?"; self.label_ids.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(
                            self.label_ids
                                .iter()
                                .map(|label| Box::new(label.clone()) as Box<dyn ToSql + Send>),
                        )
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        for label_id in &mut self.label_ids {
            interface
                .execute(
                    format!(
                        r#"
                INSERT OR IGNORE INTO
                    message_labels (local_message_id, local_label_id)
                VALUES
                    (?, (SELECT local_id FROM {} WHERE remote_id=? LIMIT 1))
                "#,
                        Label::table_name()
                    ),
                    params![self.local_id, label_id.clone()],
                )
                .await?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        if !self.attachments_metadata.is_empty() {
            let local_ids = {
                // Create attachment from partial metadata present in a message.
                // If attachment record already exists, only the message ids and the
                // address id are updated.
                // If no record exists we create a new one.
                let mut result = Vec::with_capacity(self.attachments_metadata.len());
                for metadata in &self.attachments_metadata {
                    let mut attachment = Attachment::find_first(
                        "WHERE remote_id = ?",
                        params![metadata.remote_id.clone()],
                        interface,
                    )
                    .await?
                    .unwrap_or(Attachment::from(metadata.clone()));

                    attachment.local_address_id = Some(self.local_address_id);
                    attachment.remote_address_id = Some(self.remote_address_id.clone());
                    attachment.local_message_id = self.local_id;
                    attachment.remote_message_id = self.remote_id.clone();
                    attachment.save_using(interface).await?;

                    let local_id = attachment.local_id.expect("Should be set");

                    interface
                        .execute(
                            "INSERT OR IGNORE INTO message_attachments VALUES (?,?)",
                            params![self.local_id.unwrap(), local_id],
                        )
                        .await?;

                    result.push(local_id);
                }
                result
            };

            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    message_attachments
                WHERE
                    local_message_id = ?
                    AND local_attachment_id NOT IN ({})
                ",
                        vec!["?"; local_ids.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(
                            local_ids
                                .iter()
                                .map(|attachment| Box::new(*attachment) as Box<dyn ToSql + Send>),
                        )
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    message_attachments
                WHERE
                    local_message_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        Ok(())
    }

    /// TODO: Document this method.
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.label_ids.iter().any(|l| *l == LabelId::starred())
    }

    /// Get message from remote and decrypt it
    pub async fn decrypt_from_remote<P: PgpProviderSync, PM: ProtonMail, A>(
        &self,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
        interface: &A,
    ) -> Result<DecryptedMessageBody, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        // Fetch metadata first to sync contents and cache.
        let encrypted_msg = self.sync_message_body(api, interface).await?;

        // TODO: Verify signature.
        let (decrypted_body, _) = encrypted_msg
            .decrypt(&pgp_provider, &address_keys)
            .with_context(|| {
                format!(
                    "Failed to decrypt message for localid ({:?})",
                    self.local_id
                )
            })?;

        match decrypted_body {
            DecryptedBody::Plain(body) => Ok(DecryptedMessageBody {
                metadata: encrypted_msg.metadata,
                body,
                pgp_attachments: None,
                pgp_subject: None,
            }),
            DecryptedBody::Mime(ProcessedMessage {
                body,
                attachments,
                encrypted_subject,
                ..
            }) => Ok(DecryptedMessageBody {
                metadata: encrypted_msg.metadata,
                body,
                pgp_attachments: Some(attachments),
                pgp_subject: encrypted_subject,
            }),
        }
    }

    /// Given a list of message metadata check if there are any missing dependencies like
    /// undownloaded labels or addresses.
    ///
    ///
    /// # Parameters
    ///
    /// * `messages`  - The messages to check.
    /// * `api`       - The API instance to use.
    /// * `stash`     - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    async fn sync_dependencies_from_metadata<A>(
        messages: &[MessageMetadata],
        api: &Proton,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut addrs = vec![];
        // First we load the addresses because the addresses need to exist before the messages get
        // loaded.
        for msg in messages {
            if (Address::find_by_id(RemoteId::from(msg.address_id.to_owned()), interface).await?)
                .is_none()
            {
                debug!("Address {} not found, syncing...", msg.address_id);
                let addr = api
                    .get_address_by_id(msg.address_id.to_owned())
                    .await?
                    .address;
                addrs.push(Address::from(addr));
            }
        }

        let tx = interface.transaction().await?;
        for mut addr in addrs {
            addr.save_using(&tx).await?;
        }
        tx.commit().await?;

        let mut missing_labels = vec![];
        for msg in messages {
            for rid in &msg.label_ids {
                // let api_rid = rid.to_owned().into();
                if (Label::find_by_id(RemoteId::from(rid.as_str()), interface))
                    .await?
                    .is_none()
                {
                    missing_labels.push(rid.clone());
                }
            }
        }

        if !missing_labels.is_empty() {
            info!(
                "{} label(s) were in a conversations but not locally, synchronizing...",
                missing_labels.len()
            );
            Label::sync_labels_by_ids(api, interface, missing_labels).await?;
        }

        Ok(())
    }

    /// Search for messages.
    ///
    /// This function accepts search options and calls the API to find any
    /// messages that fit the criteria. It operates globally and is not based on
    /// a particular mailbox; this restriction can be applied via the options.
    ///
    /// # Parameters
    ///
    /// * `options` - The search options to use.
    /// * `api`     - The API instance to use.
    /// * `stash`   - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database. Can also return an error if the found message
    /// cannot be loaded, although this would indicate a significant problem.
    ///
    pub async fn search(
        options: GetMessagesOptions,
        api: &Proton,
        stash: &Stash,
    ) -> Result<Vec<Message>, AppError> {
        let messages = api
            .get_messages(options)
            .await
            .context("Error fetching the messages from the API")?
            .messages
            .into_iter()
            .collect_vec();

        // First we load the addresses because the addresses need to exist before the messages get
        // loaded.
        Self::sync_dependencies_from_metadata(&messages, api, stash).await?;

        let mut messages =
            Self::create_or_update_messages_from_metadata_vec(messages, stash).await?;
        messages.sort_unstable_by(|x, y| {
            x.time
                .cmp(&y.time)
                .then(x.display_order.cmp(&y.display_order).reverse())
        });

        Ok(messages)
    }

    /// Synchronize the first `count` messages of the label with `label_id`.
    ///
    /// # Parameters
    ///
    /// * `label_id`  - The ID of the label to sync.
    /// * `count`     - TODO: Document this parameter.
    /// * `api`       - The API instance to use.
    /// * `stash`     - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_first_message_page<PM: ProtonMail>(
        label_id: LabelId,
        count: usize,
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let response = api
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                label_id: Some(vec![label_id.into()]),
                page: 0,
                page_size: count.min(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} messages TOTAL={}",
            response.messages.len(),
            response.total
        );

        let tx = stash.transaction().await?;
        Self::create_or_update_messages_from_metadata(response.messages, &tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Synchronize the message body.
    ///
    /// # Parameters
    ///
    /// * `cache_path` - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    /// * `interface`  - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                  to use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns error if the API request failed or the data could not be written
    /// to the database.
    ///
    pub async fn sync_message_body<PM: ProtonMail, A>(
        &self,
        api: &PM,
        interface: &A,
    ) -> Result<EncryptedMessageBody, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (metadata, body) = self.sync_message_metadata(api, interface).await?;
        let encrypted_body = if let Some(body) = body {
            body
        } else {
            self.get_api_message(api).await?.body
        };
        Ok(EncryptedMessageBody {
            encrypted_body,
            metadata,
        })
    }

    /// Sync message metadata
    ///
    /// # Parameters
    ///
    /// * `api`       - The API instance to use.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns error if the API request failed or the data could not be written
    /// to the database.
    ///
    async fn sync_message_metadata<PM: ProtonMail, A>(
        &self,
        api: &PM,
        interface: &A,
    ) -> Result<(MessageBodyMetadata, Option<String>), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mdata = if let Some(metadata) = self.get_message_body_metadata(interface).await? {
            (metadata, None)
        } else {
            let message = self.get_api_message(api).await?;

            let (metadata, body) = MessageBodyMetadata::save_from_api_data(
                message,
                Some(self.local_id.unwrap()),
                interface,
            )
            .await?;
            (metadata, Some(body))
        };
        Ok(mdata)
    }

    /// Get message body metadata from DB.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    async fn get_message_body_metadata<A>(
        &self,
        interface: &A,
    ) -> Result<Option<MessageBodyMetadata>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(
            MessageBodyMetadata::for_message(self.local_id.unwrap(), interface)
                .await
                .inspect_err(|e| error!("Failed to retrieve message body metadata from db: {e}"))?,
        )
    }

    /// Get message from remote
    async fn get_api_message(&self, api: &impl ProtonMail) -> Result<ApiMessage, AppError> {
        // metadata is not there it is either missing or the message does not exist.
        let remote_id = self
            .remote_id
            .clone()
            .ok_or(AppError::MessageHasNoRemoteId(
                self.local_id.unwrap_or(LocalId::from(0)),
            ))?;
        // sync the message body
        Ok(api.get_message(remote_id.into()).await.map(|v| v.message)?)
    }

    /// Get the available actions for messages excluding move to the current view.
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn available_actions<A>(
        view: Label,
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<MessageAvailableActions, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        let messages = Message::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;

        let mut starred = true;
        let mut deleted = true;
        let mut unread = false;
        let mut reply_all = false;

        for message in messages.iter() {
            if !message.is_starred() {
                starred = false;
            }
            if !message.deleted {
                deleted = false;
            }
            if message.unread {
                unread = true;
            }
            if message.reply_tos.value.len() > 1 {
                reply_all = true;
            }
        }

        let mut message_actions = vec![
            if starred {
                MessageAction::Unstar
            } else {
                MessageAction::Star
            },
            if unread {
                MessageAction::MarkRead
            } else {
                MessageAction::MarkUnread
            },
            // Statics
            MessageAction::Pin,
            MessageAction::LabelAs,
        ];

        if !deleted {
            message_actions.push(MessageAction::Delete);
        }

        let all_system = Label::find_by_kind(LabelType::System, interface).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let move_actions = MoveAction::vec(all_system_excluding_view, |_is_label_selected| false);
        let reply_actions = if reply_all {
            ReplyAction::all()
        } else {
            ReplyAction::single_address()
        };

        Ok(MessageAvailableActions::builder()
            .move_actions(MoveAction::system(move_actions))
            .reply_actions(reply_actions)
            .message_actions(message_actions)
            .build())
    }

    /// Get the available `label as` actions for conversations
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn available_label_as_actions<A>(
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<LabelAsAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        let all_label_as = Label::find_by_kind(LabelType::Label, interface).await?;
        let messages = Message::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;
        let all_label_as_actions = messages
            .iter()
            .flat_map(|message| {
                LabelAsAction::vec(all_label_as.iter(), |label| {
                    message
                        .custom_labels
                        .iter()
                        .map(|label| Some(label.local_id))
                        .contains(&label.local_id)
                })
            })
            .collect_vec();

        Ok(LabelAsAction::finalize(all_label_as_actions))
    }

    /// Get the available move actions for messages.
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    pub async fn available_move_to_actions<A>(
        view: Label,
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<MoveAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        let all_system = Label::find_by_kind(LabelType::System, interface).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let all_custom_folders = Label::find_by_kind(LabelType::Folder, interface).await?;
        let messages = Message::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;
        let all_move_to_actions = messages
            .iter()
            .flat_map(|message| {
                MoveAction::vec(
                    all_system_excluding_view
                        .clone()
                        .chain(all_custom_folders.iter()),
                    |label| {
                        message
                            .label_ids
                            .iter()
                            .map(Some)
                            .contains(&label.remote_id.as_ref())
                    },
                )
            })
            .collect_vec();

        MoveAction::finalize(all_move_to_actions, interface).await
    }

    /// Gets the body of a message from a message id.
    ///
    /// This will attempt to fetch the message data from the servers if it has
    /// not yet been downloaded before.
    ///
    /// # Errors
    ///
    /// - if the message failed to download
    /// - if the db query failed
    /// - if the message body could not be written to the cache
    /// - if a message with the given id could not be found
    #[tracing::instrument(level=tracing::Level::DEBUG,skip(user_context))]
    pub async fn message_body(
        user_context: &MailUserContext,
        id: LocalId,
    ) -> MailContextResult<DecryptedMessageBody> {
        let cache = user_context.messages_cache();
        let saved_message = Message::load(id, user_context.user_stash())
            .await?
            .ok_or(AppError::MessageMissing(id))?;

        let pgp_provider = proton_crypto::new_pgp_provider();
        let address_id = saved_message.remote_address_id.clone();
        let address_keys = user_context
            .unlocked_address_keys(&pgp_provider, &address_id)
            .await?;
        let api = user_context.session().api();

        Ok(saved_message
            .fetch_message_body(
                cache,
                address_keys,
                pgp_provider,
                api,
                user_context.user_stash(),
            )
            .await?)
    }

    /// Get the message's body.
    ///
    /// This will attempt to fetch the message data from the servers if it has
    /// not yet been downloaded before.
    ///
    /// # Parameters
    ///
    /// * `cache_path`   - TODO: Document this parameter.
    /// * `address_keys` - The address keys to use for decryption.
    /// * `pgp_provider` - The PGP provider to use for decryption.
    /// * `api`          - The API instance to use.
    /// * `interface`    - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                    to use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns error if the message failed to download, the db query failed or
    /// the message body could not be written to the cache.
    ///
    pub async fn fetch_message_body<P: PgpProviderSync, PM: ProtonMail, A>(
        &self,
        cache: &ProtonCache<CacheMessageConfig>,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
        interface: &A,
    ) -> Result<DecryptedMessageBody, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let key = CacheMessageKey::from_message(self, interface);

        // FIXME: https://jira.protontech.ch/projects/ET/issues/ET-1070
        // Recover from cache issues by requesting the data again.
        let file_path: PathBuf = cache
            .get_path_or_insert(
                &key,
                self.store_message_body(&key, address_keys, pgp_provider, api, interface),
            )
            .await?;

        let mut file = File::open(file_path)?;
        let mut body = String::new();
        file.read_to_string(&mut body)?;
        let metadata = self.get_message_body_metadata(interface).await?;
        let metadata = metadata.ok_or(AppError::MessageBodyMetadataMissing(
            self.local_id.expect("Should be set"),
        ))?;
        Ok(DecryptedMessageBody {
            body,
            metadata,
            pgp_attachments: None,
            pgp_subject: None,
        })
    }

    /// Fetch, decrypt and store message body in cache.
    async fn store_message_body<P: PgpProviderSync, PM: ProtonMail, A>(
        &self,
        key: &CacheMessageKey,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
        interface: &A,
    ) -> CacheResult<Vec<u8>>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let decrypted_message_body = self
            .decrypt_from_remote(address_keys, pgp_provider, api, interface)
            .await
            .map_err(|e| CacheError::Callback(anyhow!("Message decryption failed: {e}")))?;

        // FIXME: We're not caching the fully encrypted messages
        // https://jira.protontech.ch/projects/ET/issues/ET-1071
        if decrypted_message_body.pgp_attachments.is_some() {
            return Err(CacheError::Callback(anyhow!(
                "Multipart message not handled"
            )));
        }
        key.set_cached()
            .await
            .map_err(|e| CacheError::Callback(anyhow!("Couldn't set message as cached: {e}")))?;
        Ok(decrypted_message_body.body.into_bytes())
    }

    /// Finds all messages that have expired and deletes them.
    pub async fn delete_expired<A>(interface: &A) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let ids = Self::find_local_ids(
            r"
        WHERE
          expiration_time < STRFTIME('%s', 'NOW')
          AND expiration_time != 0
        ",
            vec![],
            interface,
        )
        .await?;
        Self::mark_deleted(ids, interface).await
    }

    /// Mark the messages with `ids` as read.
    ///
    /// This method also updates all the label counters and conversation labels
    /// where these messages belong to.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fails.
    pub async fn mark_read<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Self::mark_read_or_unread(true, ids, interface).await
    }

    /// Mark the messages with `ids` as unread.
    ///
    /// This method also updates all the label counters and conversation labels
    /// where these messages belong to.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fails.
    pub async fn mark_unread<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Self::mark_read_or_unread(false, ids, interface).await
    }

    async fn mark_read_or_unread<A>(
        mark_read: bool,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        struct IdPair {
            local_message_id: LocalId,
            local_conversation_id: LocalId,
        }

        let ids = ids.into_iter();

        let mut updated: Vec<IdPair> = Vec::with_capacity(ids.size_hint().1.unwrap_or(0));

        // update unread flag
        for id in ids {
            if let Some(mut message) = Message::find_first(
                "WHERE local_id=? AND unread=?",
                params![id, if mark_read { 1 } else { 0 }],
                interface,
            )
            .await?
            {
                message.unread = !mark_read;
                message.save_using(interface).await?;
                updated.push(IdPair {
                    local_message_id: message.local_id.unwrap(),
                    local_conversation_id: message.local_conversation_id.unwrap(),
                });
            }
        }

        if updated.is_empty() {
            // Nothing was changed.
            return Ok(());
        }

        // Publish updates for all affected ids.

        // Messages Counters
        for id_pair in &updated {
            let labels = Label::find(
                indoc! {"
                        WHERE local_id IN (
                            SELECT local_label_id FROM message_labels
                            WHERE local_message_id=?
                         )"},
                params![id_pair.local_message_id],
                interface,
                None,
            )
            .await?;
            for mut label in labels {
                if mark_read {
                    label.unread_msg -= 1;
                } else {
                    label.unread_msg += 1;
                }

                label.save_using(interface).await?
            }
        }

        let mut label_ids = BTreeSet::new();
        // Update conversation labels
        for id_pair in &updated {
            let mut conversation_labels = ConversationLabel::find(
                indoc! {
                "WHERE local_conversation_id=? AND local_label_id IN (
                    SELECT local_label_id FROM message_labels WHERE local_message_id=?
                )"},
                params![id_pair.local_conversation_id, id_pair.local_message_id],
                interface,
                None,
            )
            .await?;
            for conversation_label in &mut conversation_labels {
                if mark_read {
                    conversation_label.context_num_unread -= 1;

                    if conversation_label.context_num_unread == 0 {
                        label_ids.insert(conversation_label.local_label_id.unwrap());
                    }
                } else {
                    conversation_label.context_num_unread += 1;

                    if conversation_label.context_num_unread == 1 {
                        label_ids.insert(conversation_label.local_label_id.unwrap());
                    }
                }
                conversation_label.save_using(interface).await?
            }
        }

        for label_id in label_ids {
            // Update conversation label counts.
            if let Some(mut label) = Label::find_by_id(label_id, interface).await? {
                if mark_read {
                    label.unread_conv -= 1;
                } else {
                    label.unread_conv += 1;
                }
                label.save_using(interface).await?;
            }
        }

        Ok(())
    }

    /// Converts an [`ApiMessage`] into a [`Message`].
    ///
    /// # Parameters
    ///
    /// * `value`     - The [`ApiMessage`] to convert.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    pub async fn from_api_data<A>(value: ApiMessage, interface: &A) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Message::from_api_metadata(value.metadata, interface).await
    }

    /// Converts an [`ApiMessageMetadata`] into a [`Message`].
    ///
    /// # Parameters
    ///
    /// * `value`     - The [`ApiMessage`] to convert.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    pub async fn from_api_metadata<A>(
        value: ApiMessageMetadata,
        interface: &A,
    ) -> Result<Self, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_ids: Vec<LabelId> = value.label_ids.into_iter().map_into().collect();

        Ok(Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            local_conversation_id: None,
            remote_conversation_id: Some(value.conversation_id.into()),
            local_address_id: RemoteId::from(value.address_id.clone())
                .counterpart::<Address, _>(interface)
                .await?
                .ok_or_else(|| {
                    AppError::LocalIdNotFound("Address".to_owned(), value.address_id.clone().into())
                })?,
            remote_address_id: value.address_id.into(),
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(AttachmentMetadata::from)
                .collect(),
            bcc_list: MessageAddresses {
                value: value.bcc_list.into_iter().map(|v| v.into()).collect(),
            },
            cc_list: MessageAddresses {
                value: value.cc_list.into_iter().map(|v| v.into()).collect(),
            },
            deleted: false,
            display_order: value.order,
            expiration_time: value.expiration_time,
            external_id: value.external_id.map(|v| v.into()),
            flags: value.flags.into(),
            is_forwarded: value.is_forwarded,
            is_replied: value.is_replied,
            is_replied_all: value.is_replied_all,
            exclusive_location: None,
            label_ids,
            num_attachments: value.num_attachments,
            reply_tos: MessageAddresses {
                value: value.reply_tos.into_iter().map(|v| v.into()).collect(),
            },
            sender: value.sender.into(),
            size: value.size,
            snooze_time: value.snooze_time,
            subject: value.subject,
            time: value.time,
            to_list: MessageAddresses {
                value: value.to_list.into_iter().map(|v| v.into()).collect(),
            },
            unread: value.unread,
            cached: false,
            row_id: None,
            stash: Some(interface.stash().to_owned()),
            custom_labels: vec![],
        })
    }

    /// Apply label with `local_label_id` to the given messages with `ids`.
    ///
    /// This will also update conversation labels and label counters.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fail.
    pub async fn apply_label<A>(
        local_label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut conversation_messages = BTreeMap::<LocalId, Vec<LocalId>>::new();

        for id in ids {
            if match interface
                .query_value::<_, LocalId>(
                    "INSERT OR IGNORE INTO message_labels VALUES (?,?) RETURNING local_message_id AS value",
                    params![id, local_label_id],
                )
                .await
            {
                Ok(_) => true,
                Err(e) => {
                    if !matches!(
                        e,
                        StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                    ) {
                        return Err(e);
                    }
                    false
                }
            } {
                if let Some(message) = Message::find_by_id(id, interface).await? {
                    match conversation_messages.entry(message.local_conversation_id.unwrap()) {
                        Entry::Vacant(v) => {
                            v.insert(vec![id]);
                        }
                        Entry::Occupied(mut o) => {
                            o.get_mut().push(id);
                        }
                    }
                }
            }
        }

        if conversation_messages.is_empty() {
            // Nothing to do.
            return Ok(());
        }

        for (conversation_id, message_ids) in conversation_messages {
            Conversation::label_impl(local_label_id, conversation_id, &message_ids, interface)
                .await?;
        }

        Ok(())
    }

    /// Remove label with `local_label_id` to the given messages with `ids`.
    ///
    /// This will also update conversation labels and label counters.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fail.
    pub async fn remove_label<A>(
        local_label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut unread_count = 0_u64;
        let mut updated_count = 0_u64;
        let mut conversation_messages = BTreeMap::<LocalId, Vec<LocalId>>::new();

        for id in ids {
            let id = match interface.query_value::<_,LocalId>(
                "DELETE FROM message_labels WHERE local_label_id=? AND local_message_id=? RETURNING local_message_id AS value",
                params![local_label_id, id],
            ).await {
                Ok(v) => v,
                Err(e) => {
                    if !matches!(e, StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) {
                        return Err(e)
                    }
                    continue;
                }
            };

            let message = Message::find_by_id(id, interface)
                .await?
                .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;

            match conversation_messages.entry(message.local_conversation_id.unwrap()) {
                Entry::Vacant(v) => {
                    v.insert(vec![id]);
                }
                Entry::Occupied(mut o) => {
                    o.get_mut().push(id);
                }
            }

            if message.unread {
                unread_count += 1;
            }

            updated_count += 1;
        }

        if conversation_messages.is_empty() {
            // nothing to do.
            return Ok(());
        }

        for (conversation_id, message_ids) in conversation_messages {
            let (remaining_unread, remaining_messages): (u64, u64) =
                match ConversationMessageLabelStats::without(
                    conversation_id,
                    local_label_id,
                    &message_ids,
                    interface,
                )
                .await
                {
                    Ok(stats) => {
                        let mut conversation_label = ConversationLabel::find_first(
                            "WHERE local_conversation_id=? AND local_label_id=?",
                            params![conversation_id, local_label_id],
                            interface,
                        )
                        .await?
                        .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;
                        conversation_label.context_time = stats.time;
                        conversation_label.context_snooze_time = stats.snooze_time;
                        conversation_label.context_expiration_time = stats.expiration_time;
                        conversation_label.context_size = stats.size;
                        conversation_label.context_num_messages = stats.count;
                        conversation_label.context_num_attachments = stats.num_attachments as u64;
                        conversation_label.save_using(interface).await?;
                        (
                            conversation_label.context_num_unread,
                            conversation_label.context_num_messages,
                        )
                    }
                    Err(e) => {
                        if !matches!(
                            e,
                            StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                        ) {
                            return Err(e);
                        }
                        // If no information is returned it means there are no messages associated
                        // with this label.
                        interface.execute("DELETE FROM conversation_labels WHERE local_conversation_id=? AND local_label_id=?", params![conversation_id,local_label_id]).await?;
                        (0, 0)
                    }
                };

            let mut label = Label::find_by_id(local_label_id, interface)
                .await?
                .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;

            // update conversation counters
            if remaining_unread == 0 || remaining_messages == 0 {
                if remaining_unread == 0 && unread_count != 0 {
                    label.unread_conv -= 1;
                }
                if remaining_messages == 0 {
                    label.total_conv -= 1;
                }
            }

            // update message counters
            label.unread_msg -= unread_count;
            label.total_msg -= updated_count;

            label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Watch a message with `local_id` for changes.
    ///
    /// Returns `None` if the message could not be found.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn watch_message<A>(
        local_id: LocalId,
        interface: &A,
    ) -> Result<Option<(Message, flume::Receiver<()>)>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        //TODO(ET-1088): Return ResultSetChange<Message> instead of ()
        let (msg_sender, msg_receiver) = flume::unbounded();
        let (label_sender, label_receiver) = flume::unbounded();
        let (cb_sender, cb_receiver) = flume::unbounded();

        let (mut message, _) = futures::try_join!(
            Message::find(
                "WHERE local_id=? AND messages.deleted = 0",
                params![local_id],
                interface,
                Some(msg_sender),
            ),
            Label::find(
                formatdoc!(
                    "
                WHERE label_type=? AND local_id IN (
                    SELECT local_label_id FROM message_labels WHERE local_message_id=?
                )
            "
                ),
                params![LabelType::Label, local_id],
                interface,
                Some(label_sender)
            )
        )?;

        if message.is_empty() {
            return Ok(None);
        }

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    label_result = label_receiver.recv_async() =>  {
                        if label_result.is_err() {
                            return;
                        }
                        if cb_sender.send_async(()).await.is_err() {
                            return;
                        }
                    }
                    msg_result = msg_receiver.recv_async() => {
                        if msg_result.is_err() {
                            return;
                        }
                        if cb_sender.send_async(()).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok(Some((message.swap_remove(0), cb_receiver)))
    }

    /// Watch all messages in the label with `local_label_id` for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn watch_in_label<A>(
        local_label_id: LocalId,
        interface: &A,
    ) -> Result<(Vec<Message>, flume::Receiver<()>), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        //TODO(ET-1088): Return ResultSetChange<Message> instead of ()
        let (msg_sender, msg_receiver) = flume::unbounded();
        let (label_sender, label_receiver) = flume::unbounded();
        let (cb_sender, cb_receiver) = flume::unbounded();

        let (messages, _) = futures::try_join!(
            Message::in_label(local_label_id, interface, Some(msg_sender)),
            Label::find(
                formatdoc!(
                    "
                WHERE label_type=? AND local_id IN (
                    SELECT local_label_id FROM message_labels WHERE local_message_id IN (
                        SELECT local_message_id FROM message_labels WHERE local_label_id=?
                    )
                ) ORDER BY display_order ASC
            "
                ),
                params![LabelType::Label, local_label_id],
                interface,
                Some(label_sender)
            )
        )?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    label_result = label_receiver.recv_async() =>  {
                        if label_result.is_err() {
                            return;
                        }
                        if cb_sender.send_async(()).await.is_err() {
                            return;
                        }
                    }
                    msg_result = msg_receiver.recv_async() => {
                        if msg_result.is_err() {
                            return;
                        }
                        if cb_sender.send_async(()).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok((messages, cb_receiver))
    }

    /// Retrieve all the messages which are in a given label.
    ///
    /// # Params
    ///
    /// * `local_label_id` - Label where to search in
    /// * `interface`      - Connection to the database
    /// * `queue`          - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn in_label<A>(
        local_label_id: LocalId,
        interface: &A,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Message::find(
            formatdoc!(
                "
                JOIN message_labels
                    ON messages.local_id = message_labels.local_message_id
                WHERE
                    message_labels.local_label_id = ?
                    AND messages.deleted = 0
                ORDER BY messages.time DESC, display_order DESC
                "
            ),
            params![local_label_id],
            interface,
            queue,
        )
        .await
    }

    /// Get all messages which belong to the conversation with
    /// `local_conversation_id`.
    ///
    /// # Params
    ///
    /// * `local_conversation_id` - Conversation id to which the messages belong
    ///                             to.
    /// * `interface`             - Connection to the database.
    /// * `queue`                 - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed
    pub async fn in_conversation<A>(
        local_conversation_id: LocalId,
        interface: &A,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Message::find(
            "WHERE local_conversation_id = ? AND messages.deleted = 0 ORDER BY time ASC, display_order ASC",
            params![local_conversation_id],
            interface,
            queue,
        )
        .await
    }

    /// Create a paginator for messages in a given label.
    ///
    /// # Params
    ///
    /// * `context`        - Active user context.
    /// * `local_label_id` - Label where to paginate in.
    /// * `page_count`     - Number of elements per page.
    /// * `queue`          - Optional subscriber for changes.
    /// * `filter`         - Filter options for pagination.
    /// * `options`        - Search options for pagination.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn paginate_in_label(
        context: &MailUserContext,
        local_label_id: LocalId,
        page_count: u32,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
        filter: PaginatorFilter,
        options: PaginatorSearchOptions,
    ) -> Result<PaginatorCompat<Self, MessageDataSource>, AppError> {
        let remote_source =
            MessageDataSource::new(context, local_label_id, filter.clone(), options.clone())
                .await?;
        let mut conditions = vec!["messages.deleted = 0".to_owned()];

        if let Some(unread) = filter.unread {
            conditions.push(format!("messages.unread = {}", if unread { 1 } else { 0 }));
        }
        if let Some(keywords) = options.keywords {
            let mut keyword_conditions = Vec::new();
            for word in keywords.split_whitespace() {
                keyword_conditions.push(formatdoc!(
                    "(
                        messages.subject LIKE '%{word}%' OR
                        messages.to_list LIKE '%{word}%' OR
                        messages.sender LIKE '%{word}%'
                    )"
                ));
            }
            if !keyword_conditions.is_empty() {
                conditions.push(keyword_conditions.join(" AND "));
            }
        }

        let query = formatdoc!(
            "
            JOIN message_labels
                ON messages.local_id = message_labels.local_message_id
            WHERE
                message_labels.local_label_id = ?
                AND {}
            ORDER BY
                messages.time DESC
            ",
            conditions.join(" AND ")
        );

        let params = vec![Param::Integer(
            i64::try_from(local_label_id.as_u64()).map_err(|err| {
                StashError::ExecutionError(SqliteError::ToSqlConversionFailure(Box::new(err)))
            })?,
        )];

        Ok(PaginatorCompat::new(
            Paginator::new(
                query,
                params,
                context.user_stash(),
                NonZeroU32::new(page_count)
                    .ok_or(StashError::Custom("Invalid Page Count value".to_owned()))?,
                remote_source,
                queue,
            )
            .await?,
        ))
    }

    /// This fn should be called for message endpoints.
    /// Repeatedly calls `endpoint` in batches of 150 in parallel.
    async fn split_request<F, Fut>(
        ids: impl IntoIterator<Item = RemoteId>,
        endpoint: F,
    ) -> Result<Vec<OperationResult>, ApiServiceError>
    where
        F: Fn(Vec<ApiRemoteId>) -> Fut,
        Fut: Future<Output = Result<Vec<OperationResult>, ApiServiceError>>,
    {
        split_request(ids, 150, endpoint).await
    }

    /// Update message counters for `messages` after being marked as deleted.
    async fn update_message_counters_after_soft_delete<A>(
        messages: impl IntoIterator<Item = Message>,
        interface: &A,
    ) -> Result<HashMap<LocalId, MessageLabelStats>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_stats = MessageLabelStats::build(messages, interface).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                label.total_msg -= stats.count;
                label.unread_msg -= stats.unread_count;
                label.save_using(interface).await?;
            }
        }

        Ok(label_stats)
    }

    /// Update message counters for `messages` after being unmarked as deleted.
    async fn update_message_counters_after_soft_undelete<A>(
        messages: impl IntoIterator<Item = Message>,
        interface: &A,
    ) -> Result<HashMap<LocalId, MessageLabelStats>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let label_stats = MessageLabelStats::build(messages, interface).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                label.total_msg += stats.count;
                label.unread_msg += stats.unread_count;
                label.save_using(interface).await?;
            }
        }

        Ok(label_stats)
    }

    /// Get the possible next display order.
    ///
    /// Finds the maximum display order value in all messages and adds 1
    /// to the existing value.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    ///
    pub async fn next_display_order<A>(interface: &A) -> Result<u64, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(interface
            .query_value::<_, u64>(
                format!(
                    "SELECT IFNULL(MAX(display_order),0) AS value FROM {}",
                    Self::table_name()
                ),
                vec![],
            )
            .await?
            .saturating_add(1))
    }

    /// Only get Disposition::Attachment attachments
    pub fn get_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Attachment))
            .cloned()
            .collect()
    }

    /// Only get Disposition::Inline attachments
    #[allow(dead_code)] // Will get used later on
    fn get_inline_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Inline))
            .cloned()
            .collect()
    }
}

#[derive(Debug)]
struct MessageLabelStats {
    pub unread_count: u64,
    pub count: u64,
    pub attachment_count: u64,
    pub size: u64,
}

impl MessageLabelStats {
    async fn build<A>(
        messages: impl IntoIterator<Item = Message>,
        interface: &A,
    ) -> Result<HashMap<LocalId, MessageLabelStats>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let messages = messages.into_iter();
        let mut label_stats = HashMap::with_capacity(messages.size_hint().1.unwrap_or(4));
        for message in messages {
            let label_ids = interface
                .query_values::<_, LocalId>(
                    "SELECT local_label_id AS value FROM message_labels WHERE local_message_id=?",
                    params![message.local_id.unwrap()],
                )
                .await?;
            for label_id in label_ids {
                match label_stats.entry(label_id) {
                    HmEntry::Occupied(mut o) => {
                        let details: &mut MessageLabelStats = o.get_mut();
                        details.count += 1;
                        if message.unread {
                            details.unread_count += 1;
                        }
                        details.attachment_count += message.num_attachments as u64;
                        details.size += message.size;
                    }
                    HmEntry::Vacant(v) => {
                        v.insert(MessageLabelStats {
                            count: 1,
                            unread_count: message.unread as u64,
                            attachment_count: message.num_attachments as u64,
                            size: message.size,
                        });
                    }
                }
            }
        }

        Ok(label_stats)
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            local_address_id: 0.into(),
            remote_address_id: RemoteId::new(Default::default()),
            // The rest are by default default.
            flags: Default::default(),
            local_id: Default::default(),
            remote_id: Default::default(),
            local_conversation_id: Default::default(),
            remote_conversation_id: Default::default(),
            attachments_metadata: Default::default(),
            bcc_list: Default::default(),
            cc_list: Default::default(),
            deleted: Default::default(),
            expiration_time: Default::default(),
            external_id: Default::default(),
            is_forwarded: Default::default(),
            is_replied: Default::default(),
            is_replied_all: Default::default(),
            label_ids: Default::default(),
            exclusive_location: Default::default(),
            num_attachments: Default::default(),
            display_order: Default::default(),
            reply_tos: Default::default(),
            sender: Default::default(),
            size: Default::default(),
            snooze_time: Default::default(),
            subject: Default::default(),
            time: Default::default(),
            to_list: Default::default(),
            unread: Default::default(),
            cached: false,
            custom_labels: Default::default(),
            row_id: Default::default(),
            stash: Default::default(),
        }
    }
}

/// Metadata associated with the Body of a message.
///
/// Message bodies are not stored in the database.
///
/// Note that this information does not come directly from the API, and so there
/// is no equivalent API struct to convert from. Rather, the metadata is
/// obtained from [`DecryptedMessageBody`].
///
/// For metadata associated with a message see [`MessageMetadata`].
///
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("message_bodies")]
#[ModelActions(on_load, on_save)]
pub struct MessageBodyMetadata {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(optional)]
    pub local_message_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_message_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub header: String,

    /// TODO: Document this field.
    #[DbField]
    pub mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub parsed_headers: ParsedHeaders,

    /// Attachments associated with the message body.
    pub attachments: Vec<Attachment>,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl MessageBodyMetadata {
    /// Save or update the `MessageBodyMetadata` in the database.
    ///
    /// It's imperative to call this function rather than [`Model::save()`] to make sure that the
    /// `MessageBodyMetadata` and it's corresponding `Message` share the same `id`.
    ///
    /// There is currently no way to handle this in stash directly, so we have
    /// to manually perform this check.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update the `MessageBodyMetadata` in the database.
    ///
    /// It's imperative to call this function rather than [`Model::save_using()`] to make sure that
    /// the `MessageBodyMetadata` and it's corresponding `Message` share the same `id`.
    ///
    /// There is currently no way to handle this in stash directly, so we have
    /// to manually perform this check.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if self.local_message_id.is_none() {
            if let Some(remote_id) = self.remote_message_id.clone() {
                let message =
                    Message::find_first("WHERE remote_id = ?", params![remote_id], interface)
                        .await?;
                if let Some(message) = message {
                    self.local_message_id = message.local_id;
                }
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Extends [`Model::load()`] to pre-load attachments.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    pub async fn on_load<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        self.attachments = Attachment::for_message(self.local_message_id.unwrap(), interface)
            .await
            .inspect_err(|e| error!("Failed to load attachments for body metadata: {e}"))?;

        Ok(())
    }

    /// Extends [`Model::on_save()`] to insert attachment links.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Update all attachment links - When creating drafts we can update
        // and create new ones.
        interface
            .execute(
                "DELETE FROM message_attachments WHERE local_message_id=?",
                params![self.local_message_id],
            )
            .await?;
        for attachment in &self.attachments {
            interface
                .execute(
                    "INSERT OR IGNORE INTO message_attachments (local_attachment_id, local_message_id) VALUES (?,?)",
                    params![attachment.local_id.unwrap(), self.local_message_id],
                )
                .await?;
        }
        Ok(())
    }

    /// Load a message for the message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn for_message<A>(
        local_message_id: LocalId,
        interface: &A,
    ) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        // There is no local id on this type so we can't use find_by_id.
        Self::find_first(
            "WHERE local_message_id =?",
            params![local_message_id],
            interface,
        )
        .await
    }

    /// Save the message body metadata from a `message`.
    ///
    /// If the `local_message_id` is known, it can be passed in. If `None` it
    /// will be resolved from the database.
    ///
    /// This function also takes care of updating the attachments' metadata
    /// that is present in `message` and correctly applies this information
    /// to the returned type.
    ///
    /// Returns the saved metadata and the message body.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fail.
    pub async fn save_from_api_data<A>(
        message: ApiMessage,
        local_message_id: Option<LocalId>,
        interface: &A,
    ) -> Result<(Self, String), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let attachments = Attachment::update_headers_from_api_message(&message, interface)
            .await
            .inspect_err(|e| {
                error!("Failed to update attachment headers: {e}");
            })?;

        let local_message_id = if let Some(id) = local_message_id {
            id
        } else {
            let remote_id: RemoteId = message.metadata.id.clone().into();
            remote_id
                .counterpart::<Message, _>(interface)
                .await?
                .ok_or(AppError::UnknownMessage(remote_id))?
        };

        // create message in the database and store body in the cache.
        let mut metadata = MessageBodyMetadata {
            local_message_id: Some(local_message_id),
            remote_message_id: Some(message.metadata.id.into()),
            header: message.header.clone(),
            parsed_headers: ParsedHeaders {
                headers: message.parsed_headers,
            },
            mime_type: message.mime_type.into(),
            attachments,
            row_id: None,
            stash: Some(interface.stash().clone()),
        };
        metadata
            .save_using(interface)
            .await
            .inspect_err(|e| error!("Failed to store message body metadata in db: {e}"))?;

        Ok((metadata, message.body))
    }
}

/// Calculates the combined information for a list of message that belong to a given
/// conversation and a given label.
struct ConversationMessageLabelStats {
    pub size: u64,
    pub time: u64,
    pub expiration_time: u64,
    pub count: u64,
    pub unread: u64,
    pub num_attachments: u32,
    pub snooze_time: u64,
}

impl ConversationMessageLabelStats {
    /// Get stats about for a conversation with `conversation_id` with the
    /// given `message_ids` for a label with `label_id`.
    async fn with<A>(
        conversation_id: LocalId,
        label_id: LocalId,
        message_ids: &[LocalId],
        interface: &A,
    ) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let params = [label_id, conversation_id]
            .into_iter()
            .chain(message_ids.iter().cloned())
            .map(|v| -> Box<dyn ToSql + Send> { Box::new(v) })
            .collect();
        let messages = Message::find(format!(indoc! {"
                JOIN message_labels AS ML ON ML.local_message_id = messages.local_id AND ML.local_label_id = ?
                WHERE messages.local_conversation_id = ? AND messages.local_id IN ({})
            "}, vec!["?"; message_ids.len()].join(",")),
                                     params, interface, None).await?;

        if messages.is_empty() {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        }

        Ok(Self::from_messages(&messages))
    }

    /// Get stats about for a conversation with `conversation_id` for all the
    /// message that do not match the given `message_ids` for a label with
    /// `label_id`.
    pub async fn without<A>(
        conversation_id: LocalId,
        label_id: LocalId,
        message_ids: &[LocalId],
        interface: &A,
    ) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let params = [label_id, conversation_id]
            .into_iter()
            .chain(message_ids.iter().cloned())
            .map(|v| -> Box<dyn ToSql + Send> { Box::new(v) })
            .collect();
        let messages = Message::find(format!(indoc! {"
                JOIN message_labels AS ML ON ML.local_message_id = messages.local_id AND ML.local_label_id = ?
                WHERE messages.local_conversation_id = ? AND messages.local_id NOT IN ({})
            "}, vec!["?"; message_ids.len()].join(",")),
                                     params, interface, None).await?;

        if messages.is_empty() {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        }

        Ok(Self::from_messages(&messages))
    }

    fn from_messages(messages: &[Message]) -> Self {
        let mut stats = Self {
            size: 0,
            time: 0,
            expiration_time: 0,
            count: 0,
            unread: 0,
            num_attachments: 0,
            snooze_time: 0,
        };

        for message in messages {
            stats.size += message.size;
            stats.time = stats.time.max(message.time);
            stats.expiration_time = stats.expiration_time.max(message.expiration_time);
            stats.count += 1;
            if message.unread {
                stats.unread += 1
            }
            stats.num_attachments += message.num_attachments;
            stats.snooze_time = stats.snooze_time.max(message.snooze_time);
        }

        stats
    }
}

/// A data source for a [`Paginator`] which syncs pages of [`Message`]s in
/// a [`Label`].
pub struct ConversationDataSource {
    /// Session for network request
    session: Session,

    /// Remote id of the label.
    remote_label_id: LabelId,

    /// Local id of the label.
    local_label_id: LocalId,

    /// Filter options for pagination.
    filter: PaginatorFilter,
}

impl ConversationDataSource {
    /// Create a new data source for the given `label_id`.
    ///
    /// # Parameters
    ///
    /// * `context`  - Active user context.
    /// * `label_id` - Local id of the label.
    /// * `filter`   - Filter options for pagination.
    ///
    /// # Errors
    ///
    /// Returns error if the remote id for the label can't be resolved.
    ///
    pub async fn new(
        context: &MailUserContext,
        label_id: LocalId,
        filter: PaginatorFilter,
    ) -> Result<Self, AppError> {
        let Some(remote_id) = label_id
            .counterpart::<Label, _>(context.user_stash())
            .await?
        else {
            return Err(AppError::LabelDoesNotHaveRemoteId(label_id));
        };

        Ok(Self {
            remote_label_id: remote_id.into(),
            session: context.session().clone(),
            local_label_id: label_id,
            filter,
        })
    }
}

impl DataSource for ConversationDataSource {
    type Item = Conversation;
    type Error = AppError;

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, stash))]
    async fn total(&self, stash: &Stash) -> Result<usize, Self::Error> {
        let label = Label::find_by_id(self.local_label_id, stash)
            .await?
            .ok_or(AppError::LabelNotFound(self.local_label_id))?;
        debug!("Total conversations: {}", label.total_conv);
        Ok(label.total_conv.try_into().unwrap_or(0))
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self))]
    async fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        let response = self
            .session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(self.remote_label_id.clone().into()),
                page_size: page_size.get() as u64,
                unread: self.filter.unread,
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} conversations. Total={}",
            response.conversations.len(),
            response.total
        );
        Ok(self
            .save_to_database(
                response.conversations.into_iter().map_into().collect(),
                stash,
            )
            .await?)
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, elements))]
    async fn sync_page_after(
        &self,
        _: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        if elements.is_empty() {
            warn!("No element to sync");
            return Ok(vec![]);
        }

        // Find the first last element with a valid remote id.
        let Some(last_element) = elements
            .iter()
            .rev()
            .find(|element| element.remote_id.is_some())
        else {
            return Err(AppError::NoMessageWithValidRemoteIdFoundInPage);
        };
        // Safe to unwrap as we have validated this before.
        let last_element_id: proton_api_core::services::proton::common::RemoteId =
            last_element.remote_id.clone().unwrap().into();

        debug!("Last Element= {last_element_id}");

        let Some(last_element_time) = last_element
            .labels
            .iter()
            .find(|l| l.local_label_id.unwrap() == self.local_label_id)
            .map(|v| v.context_time)
        else {
            return Err(AppError::Other(anyhow!(
                "Conversation does not have active label"
            )));
        };

        let mut response = self
            .session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                end: Some(last_element_time),
                end_id: Some(last_element_id.clone()),
                label_id: Some(self.remote_label_id.clone().into()),
                page_size: page_size.get() as u64 + 1_u64,
                unread: self.filter.unread,
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} conversations. Total={}",
            response.conversations.len(),
            response.total
        );

        // `end_id` always returns the given conversation in the search results
        // if it exists.
        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        if response.conversations[0].id == last_element_id {
            response.conversations.remove(0);
        } else if response.conversations.len() > page_size.get() as usize {
            response.conversations.pop();
        }

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        Ok(self
            .save_to_database(
                response.conversations.into_iter().map_into().collect(),
                stash,
            )
            .await?)
    }
}

impl ConversationDataSource {
    async fn save_to_database(
        &self,
        mut records: Vec<Conversation>,
        stash: &Stash,
    ) -> Result<Vec<Conversation>, StashError> {
        let tx = stash.transaction().await?;
        for record in &mut records {
            Conversation::save_using(record, &tx).await?;
        }
        tx.commit().await?;
        Ok(records)
    }
}

/// A data source for a [`Paginator`] which syncs pages of [`Message`]s in
/// a [`Label`].
pub struct MessageDataSource {
    /// Session for network request
    session: Session,

    /// Remote id of the label.
    remote_label_id: LabelId,

    /// Local id of the label.
    local_label_id: LocalId,

    /// Filter options for pagination.
    filter: PaginatorFilter,

    /// Search options for pagination.
    options: PaginatorSearchOptions,
}

impl MessageDataSource {
    /// Create a new data source for the given `label_id`.
    ///
    /// # Parameters
    ///
    /// * `context`  - Active user context.
    /// * `label_id` - Local id of the label.
    /// * `filter`   - Filter options for pagination.
    /// * `options`  - Search options for pagination.
    ///
    /// # Errors
    ///
    /// Returns error if the remote id for the label can't be resolved.
    pub async fn new(
        context: &MailUserContext,
        label_id: LocalId,
        filter: PaginatorFilter,
        options: PaginatorSearchOptions,
    ) -> Result<Self, AppError> {
        let Some(remote_id) = label_id
            .counterpart::<Label, _>(context.user_stash())
            .await?
        else {
            return Err(AppError::LabelDoesNotHaveRemoteId(label_id));
        };

        Ok(Self {
            remote_label_id: remote_id.into(),
            session: context.session().clone(),
            local_label_id: label_id,
            filter,
            options,
        })
    }
}
impl DataSource for MessageDataSource {
    type Item = Message;
    type Error = AppError;

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, stash))]
    async fn total(&self, stash: &Stash) -> Result<usize, Self::Error> {
        let label = Label::find_by_id(self.local_label_id, stash)
            .await?
            .ok_or(AppError::LabelNotFound(self.local_label_id))?;
        debug!("Total messages: {}", label.total_msg);
        Ok(label.total_msg.try_into().unwrap_or(0))
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, stash))]
    async fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        let response = self
            .session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                label_id: Some(vec![self.remote_label_id.clone().into_inner().into()]),
                page_size: page_size.get() as u64,
                unread: self.filter.unread,
                keyword: self.options.keywords.clone(),
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} messages. Total={}",
            response.messages.len(),
            response.total
        );
        let mut messages = Vec::with_capacity(response.messages.len());
        for message in response.messages {
            messages.push(Message::from_api_metadata(message, stash).await?);
        }

        Ok(self.save_to_database(messages, stash).await?)
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, stash, elements))]
    async fn sync_page_after(
        &self,
        _: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        if elements.is_empty() {
            warn!("No element to sync");
            return Ok(vec![]);
        }

        // Find the first last element with a valid remote id.
        let Some(last_element) = elements
            .iter()
            .rev()
            .find(|element| element.remote_id.is_some())
        else {
            return Err(AppError::NoMessageWithValidRemoteIdFoundInPage);
        };
        // Safe to unwrap as we have validated this before.
        let last_element_id: proton_api_core::services::proton::common::RemoteId =
            last_element.remote_id.clone().unwrap().into();

        debug!("Last Element= {last_element_id}");
        let mut response = self
            .session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                end: Some(last_element.time),
                end_id: Some(last_element_id.clone()),
                label_id: Some(vec![self.remote_label_id.clone().into_inner().into()]),
                page_size: page_size.get() as u64 + 1_u64,
                unread: self.filter.unread,
                keyword: self.options.keywords.clone(),
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} messages. Total={}",
            response.messages.len(),
            response.total
        );

        // `end_id` always returns the given message in the search results
        // if it exists.
        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        if response.messages[0].id == last_element_id {
            response.messages.remove(0);
        } else if response.messages.len() > page_size.get() as usize {
            response.messages.pop();
        }

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages = Vec::with_capacity(response.messages.len());
        for message in response.messages {
            messages.push(Message::from_api_metadata(message, stash).await?);
        }

        Ok(self.save_to_database(messages, stash).await?)
    }
}

impl MessageDataSource {
    async fn save_to_database(
        &self,
        mut records: Vec<Message>,
        stash: &Stash,
    ) -> Result<Vec<Message>, StashError> {
        let tx = stash.transaction().await?;
        for record in &mut records {
            Message::save_using(record, &tx).await?;
        }
        tx.commit().await?;
        Ok(records)
    }
}

/// Compatibility layer to map new behavior over old paginator code.
///
/// The new behavior expects all the pages to be loaded via `next_page()`
/// but in the older versions this does not happen in the first page.
///
// TODO: Remove when caching is completely implemented.
pub struct PaginatorCompat<T: Model, R: DataSource<Item = T> + 'static> {
    is_first_page: AtomicBool,
    paginator: Paginator<T, R>,
}

impl<T: Model, R: DataSource<Item = T> + 'static> PaginatorCompat<T, R> {
    fn new(paginator: Paginator<T, R>) -> Self {
        Self {
            paginator,
            is_first_page: AtomicBool::new(true),
        }
    }

    /// See [`Paginate::next_page`] for more details.
    pub async fn next_page(&self) -> Result<Vec<T>, R::Error> {
        // If it's the first time we are calling this we want the
        // current page. Otherwise we call `next_page`.
        if self.is_first_page.load(Ordering::Acquire) {
            let items = self.paginator.current_page().await?;
            self.is_first_page.store(false, Ordering::Release);
            Ok(items)
        } else {
            self.paginator.next_page().await
        }
    }

    /// See [`Paginate::result_count`] for more details.
    #[inline]
    pub async fn result_count(&self) -> u32 {
        self.paginator.result_count().await
    }

    /// See [`Paginate::has_next_page`] for more details.
    #[inline]
    pub async fn has_next_page(&self) -> bool {
        self.paginator.has_next_page().await
    }

    /// See [`Paginate::reload`] for more details.
    #[inline]
    pub async fn reload(&self) -> Result<Vec<T>, StashError> {
        self.paginator.reload().await
    }
}

/// Filter options for pagination
#[derive(Clone, Debug, Default)]
pub struct PaginatorFilter {
    /// If true, only return unread conversations/messages
    pub unread: Option<bool>,
}

/// Search options for pagination
#[derive(Clone, Debug, Default)]
pub struct PaginatorSearchOptions {
    /// Keywords to use in search.
    pub keywords: Option<String>,
}
