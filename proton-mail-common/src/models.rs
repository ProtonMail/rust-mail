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

#[cfg(test)]
#[path = "tests/models.rs"]
mod tests;

use crate::cache::CacheMessageConfig;
use crate::datatypes::{
    AlmostAllMail, AttachmentEncryptedSignature, AttachmentMetadata, AttachmentSignature,
    ComposerDirection, ComposerMode, ConversationCount, DecryptedMessageBody, Disposition,
    EncryptedMessageBody, ExclusiveLocation, KeyPackets, LabelColor, LabelType, MessageAddress,
    MessageAddresses, MessageAttachmentInfos, MessageButtons, MessageCount, MessageFlags, MimeType,
    MobileSettings, NextMessageOnMove, ParsedHeaders, PgpScheme, PmSignature, ShowImages,
    ShowMoved, SpamAction, SwipeAction, SystemLabelId, ViewLayout, ViewMode,
};
use crate::{AppError, ALL_LABEL_TYPES};
use bytes::Bytes;
use indoc::formatdoc;
use proton_action_queue::db::{ActionQueueExtension, OptionalExtension};
use proton_api_core::service::ApiServiceError;
use proton_api_mail::services::proton::requests::{
    GetConversationsOptions, GetMessagesOptions, PatchLabelRequest, PostLabelsRequest,
    PutLabelRequest,
};
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, Conversation as ApiConversation,
    ConversationLabel as ApiConversationLabel, Label as ApiLabel, MailSettings as ApiMailSettings,
    Message as ApiMessage, MessageMetadata as ApiMessageMetadata, OperationResult,
};
use proton_api_mail::services::proton::responses::{
    GetAttachmentMetadataResponse, GetMessagesResponse,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::MAX_PAGE_ELEMENT_COUNT;
use proton_core_common::cache::ProtonCache;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, DecryptableAttachment,
    KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync as PgpProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::UnlockedAddressKeys;
use smart_default::SmartDefault;
use stash::datatypes::{QueryResultString, QueryResultU64};
use stash::exports::ToSql;
use stash::macros::{DbRecord, Model};
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError, Tether};
use std::collections::HashMap;
use std::io::Read;
use tracing::{debug, error};

pub const MAIL_SETTINGS_ID: u64 = 1;

/// Represents a mail attachment.
///
/// The important thing to keep in mind for this type is that the metadata is
/// spread out through various locations. Partial metadata is available on the
/// [`Conversation`] and [`Message`] types, full information is available on the
/// [`Attachment`] type and the final piece of the information is stored in the
/// [`MessageBodyMetadata`].
///
/// To decrypt the attachment we need to sync the full [`Attachment`] type to
/// have access to the required crypto metadata. When [`Conversation`] or
/// [`Message`] is synced from the API we partially create the attachment type
/// so we can assign it a local id. This contains enough information to
/// construct the [`AttachmentMetadata`] type which is used to display contextual
/// information about the attachment.
///
/// Once we need to access an attachment we should first check if the
/// [`full metadata is present`](Attachment::has_complete_metadata) and if not
/// call [`sync`](Attachment::sync_complete_metadata). Once that has completed
/// one can use and decrypt the attachment.
///
/// Note: Extracting the last bit of information from [`MessageBodyMetadata`]
/// will come in a followup patch.
///
/// # Remarks
///
/// Do not use [`Attachment::save`] but always use
/// [`Attachment::save_or_update`].
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
    pub local_id: Option<u64>,

    /// API Attachment id.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// Address with which this attachment was encrypted.
    ///
    /// The address id can only be retrieved from a [`Message`] or the full [`Attachment`] type.
    #[DbField]
    pub remote_address_id: Option<RemoteId>,

    /// Local conversation id where this attachment is present.
    #[DbField]
    pub local_conversation_id: Option<u64>,

    /// Remote conversation id where this attachment is present.
    #[DbField]
    pub remote_conversation_id: Option<RemoteId>,

    /// Local message id where this attachment is present.
    #[DbField]
    pub local_message_id: Option<u64>,

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
    pub mime_type: MimeType,

    /// File name of the attachment.
    #[DbField]
    pub name: String,

    /// Sender of the attachment if received from an external address.
    #[DbField]
    pub sender: Option<MessageAddress>,

    /// TODO: Document this field.
    #[DbField]
    pub signature: Option<AttachmentSignature>,

    /// Size of the attachment in bytes.
    #[DbField]
    pub size: u64,

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
            name: value.name,
            sender: None,
            signature: None,
            size: value.size,
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
            name: value.name,
            size: value.size,
        }
    }
}

impl Attachment {
    /// Create attachment from partial metadata present in a `message`.
    ///
    /// If attachment record already exists, the message ids are updated. If no record exists,
    /// we create a new one.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be written to the database
    pub async fn save_from_message_metadata(
        message: &Message,
        interface: &AgnosticInterface,
    ) -> Result<Vec<u64>, StashError> {
        let mut result = Vec::with_capacity(message.attachments_metadata.len());
        for metadata in &message.attachments_metadata {
            let mut attachment = Attachment::find_first(
                "WHERE remote_id = ?",
                params![metadata.remote_id.clone()],
                interface,
            )
            .await?
            .unwrap_or(Attachment::from(metadata.clone()));

            attachment.remote_address_id = Some(message.address_id.clone());
            attachment.local_message_id = message.local_id;
            attachment.remote_message_id = message.remote_id.clone();
            attachment.save_using(interface).await?;

            let local_id = attachment.local_id.expect("Should be set");

            interface
                .execute(
                    "INSERT OR IGNORE INTO message_attachments VALUES (?,?)",
                    params![message.local_id.unwrap(), local_id],
                )
                .await?;

            result.push(local_id);
        }

        Ok(result)
    }

    /// Create attachment from partial metadata present in a `conversation`.
    ///
    /// If attachment record already exists, the conversation ids are updated. If no record exists,
    /// we create a new one.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be written to the database
    pub async fn save_from_conversation_metadata(
        conversation: &Conversation,
        interface: &AgnosticInterface,
    ) -> Result<Vec<u64>, StashError> {
        let mut result = Vec::with_capacity(conversation.attachments_metadata.len());
        for metadata in &conversation.attachments_metadata {
            let mut attachment = Attachment::find_first(
                "WHERE remote_id = ?",
                params![metadata.remote_id.clone()],
                interface,
            )
            .await?
            .unwrap_or(Attachment::from(metadata.clone()));

            attachment.local_conversation_id = conversation.local_id;
            attachment.remote_conversation_id = conversation.remote_id.clone();
            attachment.save_using(interface).await?;

            let local_id = attachment.local_id.expect("Should be set");

            interface
                .execute(
                    "INSERT OR IGNORE INTO conversation_attachments VALUES (?,?)",
                    params![conversation.local_id.unwrap(), local_id],
                )
                .await?;

            result.push(local_id);
        }

        Ok(result)
    }

    /// Load attachment metadata for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn load_conversation_attachment_metadata(
        conversation_id: u64,
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
        message_id: u64,
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
    /// It's imperative to call this function rather than [`Attachment::save`] to make sure
    /// that we override the existing partial metadata rather than create a new entry that will
    /// cause a conflict.
    ///
    /// There is currently no way to handle this in stash directly, so we have to manually perform
    /// this check.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn save_or_update(
        &mut self,
        interface: &AgnosticInterface,
    ) -> Result<(), StashError> {
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

        Self::save_using(self, interface).await
    }

    /// Retrieve the local id of an attachment based on their `remote_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn find_local_id_for_remote_id(
        remote_id: RemoteId,
        interface: &AgnosticInterface,
    ) -> Result<Option<u64>, StashError> {
        let Some(local_id) = interface
            .query::<_, QueryResultU64>(
                format!(
                    "SELECT local_id as value FROM {} WHERE remote_id=? LIMIT 1",
                    Attachment::table_name()
                ),
                params![remote_id],
            )
            .await
            .optional()?
        else {
            return Ok(None);
        };

        Ok(Some(local_id[0].value))
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
        attachment.save_or_update(interface).await?;
        *self = attachment;
        Ok(Some(()))
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
            remote_address_id: Some(value.address_id.into()),
            local_conversation_id: None,
            remote_conversation_id: Some(value.conversation_id.into()),
            local_message_id: None,
            remote_message_id: Some(value.message_id.into()),
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.clone().map(|v| v.into()),
            is_auto_forwardee: value.is_auto_forwardee,
            key_packets: Some(value.key_packets.clone().into()),
            mime_type: value.mime_type.into(),
            name: value.name,
            sender: value.sender.map(|v| v.into()),
            signature: value.signature.map(|v| v.into()),
            size: value.size,
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
    pub local_id: Option<u64>,

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

    /// TODO: Document this field.
    #[DbField]
    pub num_attachments: u64,

    /// TODO: Document this field.
    #[DbField]
    pub num_messages: u64,

    /// TODO: Document this field.
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
    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id` - TODO: Document this parameter.
    /// * `ids`      - The IDs of the conversations to label.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn apply_label_to_multiple(
        label_id: u64,
        ids: Vec<u64>,
        tether: &Tether,
    ) -> Result<(), StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        for id in ids {
            // label all conversation messages
            tether
                .execute(
                    formatdoc!(
                        r"
                WITH
                    conv_msgs
                AS (
                    SELECT local_id, ? AS label_id FROM messages WHERE local_conversation_id = ?
                )
                INSERT OR IGNORE INTO
                    message_labels (local_message_id, local_label_id)
                SELECT
                    *
                FROM
                    conv_msgs
                -- RETURNING
                --    message_id
                "
                    ),
                    params![label_id, id],
                )
                .await?;
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
        api.put_conversations_label(
            ids.into_iter().map(|id| id.into()).collect(),
            label_id.into(),
            spam_action,
        )
        .await
        .map(|r| r.responses)
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversations` - TODO: Document this parameter.
    /// * `stash`         - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn create_or_update_conversations(
        conversations: Vec<Conversation>,
        stash: &Stash,
    ) -> Result<Vec<u64>, AppError> {
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            conv.set_stash(stash);
            if let Some(existing) =
                Self::find_by_remote_id(conv.remote_id.clone().unwrap(), stash).await?
            {
                conv.local_id = existing.local_id;
                conv.row_id = existing.row_id;
                conv.stash = existing.stash;
            }
            conv.save().await?;

            ids.push(conv.local_id.unwrap());
        }
        Ok(ids)
    }

    /// Delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn delete_multiple(
        ids: Vec<u64>,
        label_id: u64,
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
                deleted = 1
            WHERE
                local_conversation_id IN ({})
                AND deleted = 0
                AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
            RETURNING
                local_id
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
        api.put_conversations_delete(
            ids.into_iter().map(|id| id.into()).collect(),
            label_id.into(),
        )
        .await
        .map(|r| r.responses)
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

    /// Find local IDs for the given remote IDs.
    ///
    /// # Parameters
    ///
    /// * `remote_ids` - The remote IDs to find local IDs for.
    /// * `tether`     - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_local_ids(
        remote_ids: Vec<RemoteId>,
        tether: &Tether,
    ) -> Result<Vec<u64>, StashError> {
        let mut ids = Vec::with_capacity(remote_ids.len());
        let query = format!(
            "SELECT local_id FROM {} WHERE remote_id = ?",
            Self::table_name()
        );
        for remote_id in remote_ids {
            if let Some(id) = tether
                .query_row::<_, QueryResultU64>(&query, params![remote_id])
                .await
                .optional()?
            {
                ids.push(id.value)
            }
        }
        Ok(ids)
    }

    /// Find remote IDs for the given local IDs.
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The local IDs to find remote IDs for.
    /// * `tether`    - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_remote_ids(
        local_ids: Vec<u64>,
        tether: &Tether,
    ) -> Result<Vec<RemoteId>, StashError> {
        let mut ids = Vec::with_capacity(local_ids.len());
        let query = format!(
            "SELECT remote_id FROM {} WHERE local_id = ? AND remote_id IS NOT NULL",
            Self::table_name()
        );

        #[derive(Debug, DbRecord, Eq, PartialEq, Clone)]
        struct Record {
            #[DbField]
            remote_id: RemoteId,
        }

        for local_id in local_ids {
            if let Some(id) = tether
                .query_row::<_, Record>(&query, params![local_id])
                .await
                .optional()?
            {
                ids.push(id.remote_id)
            }
        }
        Ok(ids)
    }

    /// Retrieve the first unread message that should be displayed to the user
    /// from the conversation's `messages`.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Parameters
    ///
    /// * `label`    - TODO: Document this parameter.
    /// * `messages` - TODO: Document this parameter.
    ///
    pub fn first_unread_message(label: &Label, messages: &[Message]) -> Option<u64> {
        if messages.is_empty() {
            return None;
        }

        fn first_consecutive_unread_msg(
            messages: &[Message],
            filter: impl Fn(&Message) -> bool,
        ) -> Option<u64> {
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
                    .find(|m| filter(m))
                    .and_then(|m| m.local_id)
            })
        }

        let view_is_starred_label_or_folder = label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id == Some(LabelId::starred());

        if view_is_starred_label_or_folder {
            first_consecutive_unread_msg(messages, |msg| !msg.flags.is_draft())
        } else {
            first_consecutive_unread_msg(messages, |msg| {
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

        let labels = Label::find(
            r#"WHERE local_id IN (SELECT local_label_id FROM conversation_labels WHERE local_conversation_id = ?)"#,
            params![self.local_id],
            interface,
            None,
        )
        .await?;

        self.exclusive_location = ExclusiveLocation::from_labels(&labels);

        self.attachments_metadata =
            Attachment::load_conversation_attachment_metadata(self.local_id.unwrap(), interface)
                .await?;

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
            let local_ids = Attachment::save_from_conversation_metadata(self, interface).await?;

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
    pub async fn mark_multiple_as_read(ids: Vec<u64>, stash: &Tether) -> Result<(), StashError> {
        for id in ids {
            if let Some(mut conv) = Conversation::load(id, stash).await? {
                conv.num_unread = 0;
                conv.save().await?;
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
        api.put_conversations_read(ids.into_iter().map(|id| id.into()).collect())
            .await
            .map(|r| r.responses)
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Parameters
    ///
    /// * `ids`    - The IDs of the conversations to mark as unread.
    /// * `tether` - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_multiple_as_unread(ids: Vec<u64>, tether: &Tether) -> Result<(), StashError> {
        // TODO: This is simplified, and will be updated when these operations are
        // TODO: refactored
        for id in ids {
            if let Some(mut conv) = Conversation::load(id, tether).await? {
                conv.num_unread = 1;
                conv.save().await?;
            }
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
        api.put_conversations_unread(ids.into_iter().map(|id| id.into()).collect())
            .await
            .map(|r| r.responses)
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id` - TODO: Document this parameter.
    /// * `ids`      - The IDs of the conversations to unlabel.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn remove_label_from_multiple(
        label_id: u64,
        ids: Vec<u64>,
        tether: &Tether,
    ) -> Result<(), StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        for id in ids {
            // label all conversation messages
            tether
                .execute(
                    formatdoc!(
                        r"
                WITH
                    conv_msgs
                AS (
                    SELECT local_id, unread FROM messages WHERE local_conversation_id = ?1
                )
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id IN (
                        SELECT local_id FROM messages WHERE local_conversation_id = ?1
                    )
                    AND message_labels.local_label_id = ?2
                RETURNING
                    local_message_id
                "
                    ),
                    params![label_id, id],
                )
                .await?;
        }
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
        api.put_conversations_unlabel(
            ids.into_iter().map(|id| id.into()).collect(),
            label_id.into(),
        )
        .await
        .map(|r| r.responses)
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
    pub async fn search<PM: ProtonMail>(
        options: GetConversationsOptions,
        api: &PM,
        stash: &Stash,
    ) -> Result<Vec<Conversation>, AppError> {
        let ids = Self::create_or_update_conversations(
            api.get_conversations(options)
                .await?
                .conversations
                .into_iter()
                .map(|c| c.into())
                .collect(),
            stash,
        )
        .await?;
        let mut conversations = vec![];
        for id in ids {
            conversations.push(
                Self::load(id, stash)
                    .await?
                    .ok_or(AppError::Other("Conversation not found".to_owned()))?,
            );
        }
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
    pub async fn star_multiple(ids: Vec<u64>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_remote_id(LabelId::starred().into(), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::apply_label_to_multiple(label_id, ids, &stash.connection()).await
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
    pub async fn unstar_multiple(ids: Vec<u64>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_remote_id(LabelId::starred().into(), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::remove_label_from_multiple(label_id, ids, &stash.connection()).await
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
        let conversation_counts = Conversation::fetch_counts(api).await?;
        let message_counts = Message::fetch_counts(api).await?;
        let tx = stash.transaction().await?;
        Label::create_or_update_conversation_counts(conversation_counts, tx.stash()).await?;
        Label::create_or_update_message_counts(message_counts, tx.stash()).await?;
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
                page_size: count.max(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        Self::create_or_update_conversations(
            response
                .conversations
                .into_iter()
                .map(|c| c.into())
                .collect(),
            stash,
        )
        .await?;
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
        ids: Vec<u64>,
        label_id: u64,
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
            RETURNING
                local_id
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
        api.put_conversations_delete(
            ids.into_iter().map(|id| id.into()).collect(),
            label_id.into(),
        )
        .await
        .map(|r| r.responses)
    }

    /// Move conversations between two labels.
    ///
    /// # Parameters
    /// * `source_id`      - Local label id where the conversations currently are.
    /// * `destination_id` - Local label id where the conversations should be moved.
    /// * `ids`            - The IDs of the conversations to move.
    /// * `tx`             - The tether to use for the database connection.
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
    pub async fn move_conversations(
        source_id: u64,
        destination_id: u64,
        ids: Vec<u64>,
        tx: &Tether,
    ) -> Result<(LabelId, LabelId), AppError> {
        let Some(source_label) = Label::load(source_id, tx).await? else {
            return Err(AppError::LabelNotFound(source_id));
        };

        let is_movable_folder = source_label.is_movable_folder();

        let Some(remote_source_id) = source_label.remote_id else {
            return Err(AppError::LabelDoesNotHaveRemoteId(source_id));
        };

        let Some(remote_destination_id) = Label::find_remote_id(destination_id, tx).await? else {
            return Err(AppError::LabelDoesNotHaveRemoteId(destination_id));
        };

        // If moving to trash, mark conversations as read.
        if remote_destination_id == LabelId::trash() {
            Conversation::mark_multiple_as_read(ids.clone(), tx)
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations as read when moving to trash: {e}");
                    e
                })?
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if remote_destination_id == LabelId::trash() || remote_destination_id == LabelId::spam() {
            let all_mail_id = Label::find_local_ids(
                vec![LabelId::all_mail()],
                &AgnosticInterface::Tether(tx.to_owned()),
            )
            .await?;
            if all_mail_id.is_empty() {
                return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_mail()));
            }

            let all_mail_local_id = all_mail_id[0];

            for &local_conversation_id in &ids {
                let label_ids =
                    ConversationLabel::labels_ids_for_conversation(local_conversation_id, tx)
                        .await?;
                for label_id in label_ids.into_iter().filter(|id| *id != all_mail_local_id) {
                    Conversation::remove_label_from_multiple(
                        label_id,
                        vec![local_conversation_id],
                        tx,
                    )
                        .await.map_err(|e| {
                        error!("Failed to remove label {label_id} from conv {local_conversation_id} when moving into spam/trash:{e}");
                        e
                    })?;
                }
            }
            // When moving out of Trash or Spam, add AlmostAllMail label
        } else if remote_source_id == LabelId::trash() || remote_source_id == LabelId::spam() {
            let almost_all_mail_id = Label::find_local_ids(
                vec![LabelId::almost_all_mail()],
                &AgnosticInterface::Tether(tx.to_owned()),
            )
            .await?;
            if almost_all_mail_id.is_empty() {
                return Err(AppError::RemoteLabelDoesNotExist(LabelId::almost_all_mail()));
            }

            let almost_all_mail_local_id = almost_all_mail_id[0];
            Conversation::apply_label_to_multiple(almost_all_mail_local_id, ids.clone(), tx)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to apply almost all mail label when moving out of spam/trash:{e}"
                    );
                    e
                })?;
        }

        if is_movable_folder {
            Conversation::remove_label_from_multiple(source_id, ids.clone(), tx).await?
        }

        Conversation::apply_label_to_multiple(destination_id, ids.clone(), tx).await?;

        Ok((remote_source_id, remote_destination_id))
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
                .map(|v| v.into())
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
            size: value.size,
            subject: value.subject,
            row_id: None,
            stash: None,
        }
    }
}

/// Contextual label metadata associated with a Conversation.
///
/// When a conversation is opened in the context of label, the
/// [`ConversationLabel`] information is superimposed over the [`Conversation`]
/// for that context.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("conversation_labels")]
pub struct ConversationLabel {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub local_conversation_id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub local_label_id: Option<u64>,

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
    pub async fn labels_ids_for_conversation(
        conversation_id: u64,
        tether: &Tether,
    ) -> Result<Vec<u64>, StashError> {
        let query = format!(
            "SELECT local_id FROM {} WHERE local_conversation_id = ?",
            Self::table_name()
        );

        Ok(tether
            .query::<_, QueryResultU64>(&query, params![conversation_id])
            .await?
            .into_iter()
            .map(|v| v.value)
            .collect())
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
            Label::find_by_remote_id(remote_label_id.clone().into(), interface).await?
        else {
            return Err(StashError::Custom(
                "Missing remote local label id".to_owned(),
            ));
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
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[ModelActions(on_save)]
#[TableName("labels")]
pub struct Label {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_parent_id: Option<u64>,

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

    pub async fn create_or_update_conversation_counts(
        counts: Vec<ConversationCount>,
        stash: &Stash,
    ) -> Result<(), StashError> {
        let tx = stash.transaction().await?;
        for count in counts {
            tx.execute(
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
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_or_update_message_counts(
        counts: Vec<MessageCount>,
        stash: &Stash,
    ) -> Result<(), StashError> {
        let tx = stash.transaction().await?;
        for count in counts {
            tx.execute(
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
        tx.commit().await?;
        Ok(())
    }

    /// TODO: Document this function.
    pub fn is_applicable_label(&self) -> bool {
        self.label_type == LabelType::Label
            || self
                .remote_id
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

    /// TODO: Document this function.
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
    pub async fn sync_labels<PM: ProtonMail>(api: &PM, stash: &Stash) -> Result<(), AppError> {
        let mut all_labels: Vec<Label> = Vec::with_capacity(64);
        for category in ALL_LABEL_TYPES {
            debug!("Fetching labels ({:?})", category);
            all_labels.extend(
                api.get_labels(category.into())
                    .await?
                    .labels
                    .into_iter()
                    .map(|l| l.into()),
            );
        }
        debug!("Storing labels into database");
        for label in all_labels.iter_mut() {
            let parent_id_option = label.remote_parent_id.clone();
            label.local_parent_id = match parent_id_option {
                Some(parent_id) => Self::find_local_ids(
                    vec![parent_id],
                    &AgnosticInterface::Stash(stash.to_owned()),
                )
                .await?
                .pop(),
                None => None,
            };
            let db_label =
                Label::find_by_remote_id(label.remote_id.clone().unwrap().into(), stash).await?;
            if let Some(mut db_label) = db_label {
                db_label.color = label.color.clone();
                db_label.display = label.display;
                db_label.expanded = label.expanded;
                db_label.initialized_conv = label.initialized_conv;
                db_label.initialized_msg = label.initialized_msg;
                db_label.name.clone_from(&label.name);
                db_label.notify = label.notify;
                db_label.path.clone_from(&label.path);
                db_label.sticky = label.sticky;
                db_label.total_conv = label.total_conv;
                db_label.total_msg = label.total_msg;
                db_label.unread_conv = label.unread_conv;
                db_label.unread_msg = label.unread_msg;
                db_label.set_stash(stash);
                db_label.save().await?;
            } else {
                label.set_stash(stash);
                label.save().await?;
            }
        }
        Ok(())
    }

    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        let parent_id_option = self.remote_parent_id.clone();
        self.local_parent_id = match parent_id_option {
            Some(parent_id) => {
                let res = Self::find_local_ids(vec![parent_id], interface)
                    .await?
                    .pop();
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
    pub fn view_mode(&self) -> Option<ViewMode> {
        let remote_id = self.remote_id.as_ref()?;

        if *remote_id == LabelId::drafts()
            || *remote_id == LabelId::sent()
            || *remote_id == LabelId::all_drafts()
            || *remote_id == LabelId::all_sent()
            || *remote_id == LabelId::all_scheduled()
        {
            return Some(ViewMode::Messages);
        }

        None
    }

    /// Find local IDs for the given remote IDs.
    ///
    /// # Parameters
    ///
    /// * `remote_ids` - The remote IDs to find local IDs for.
    /// * `tether`     - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_local_ids(
        remote_ids: Vec<LabelId>,
        interface: &AgnosticInterface,
    ) -> Result<Vec<u64>, StashError> {
        let mut ids = Vec::with_capacity(remote_ids.len());
        let query = format!(
            "SELECT local_id as value FROM {} WHERE remote_id = ?",
            Self::table_name()
        );
        for remote_id in remote_ids {
            if let Some(id) = interface
                .query::<_, QueryResultU64>(&query, params![remote_id])
                .await
                .optional()?
            {
                ids.extend(id.iter().map(|v| v.value).collect::<Vec<_>>())
            }
        }
        Ok(ids)
    }

    /// Find remote ID for the given local ID.
    ///
    /// # Parameters
    ///
    /// * `local_id` - The local ID to find remote ID.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_remote_id(
        local_id: u64,
        tether: &Tether,
    ) -> Result<Option<LabelId>, StashError> {
        let query = format!(
            "SELECT remote_id FROM {} WHERE local_id = ? AND remote_id IS NOT NULL",
            Self::table_name()
        );
        Ok(tether
            .query_row::<_, QueryResultString>(&query, params![local_id])
            .await
            .optional()?
            .map(|v| LabelId::from(v.value)))
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

#[cfg(test)]
mod default_label {
    use crate::{datatypes::LabelType, models::Label};

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
    pub local_id: Option<u64>,

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

    /// TODO: Document this field.
    #[DbField]
    pub hide_remote_images: bool,

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
        settings.save().await?;
        Ok(())
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
    pub local_id: Option<u64>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_conversation_id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_conversation_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub address_id: RemoteId,

    /// TODO: Document this field.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[DbField]
    pub bcc_list: MessageAddresses,

    /// TODO: Document this field.
    pub body: String,

    /// TODO: Document this field.
    #[DbField]
    pub cc_list: MessageAddresses,

    /// TODO: Document this field.
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

    /// TODO: Document this field.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub external_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub header: String,

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
    pub mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub num_attachments: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u64,

    /// TODO: Document this field.
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    pub parsed_headers: ParsedHeaders,

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
    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `metadata` - TODO: Document this parameter.
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn create_or_update_messages_from_metadata(
        metadata: Vec<ApiMessageMetadata>,
        stash: &Stash,
    ) -> Result<Vec<u64>, AppError> {
        let mut ids = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            let mut message = Self {
                local_id: None,
                remote_id: Some(metadata.id.into()),
                address_id: metadata.address_id.into(),
                attachments_metadata: metadata
                    .attachments_metadata
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
                bcc_list: MessageAddresses {
                    value: metadata.bcc_list.into_iter().map(|v| v.into()).collect(),
                },
                body: "".to_owned(),
                cc_list: MessageAddresses {
                    value: metadata.cc_list.into_iter().map(|v| v.into()).collect(),
                },
                deleted: false,
                display_order: metadata.order,
                expiration_time: metadata.expiration_time,
                external_id: metadata.external_id.map(|v| v.into()),
                flags: metadata.flags.into(),
                header: "".to_owned(),
                is_forwarded: metadata.is_forwarded,
                is_replied: metadata.is_replied,
                is_replied_all: metadata.is_replied_all,
                exclusive_location: None,
                label_ids: metadata.label_ids.into_iter().map(|v| v.into()).collect(),
                local_conversation_id: None,
                mime_type: MimeType::TextPlain,
                num_attachments: metadata.num_attachments,
                parsed_headers: ParsedHeaders {
                    headers: HashMap::new(),
                },
                remote_conversation_id: Some(metadata.conversation_id.into()),
                reply_tos: MessageAddresses {
                    value: metadata.reply_tos.into_iter().map(|v| v.into()).collect(),
                },
                sender: metadata.sender.into(),
                size: metadata.size,
                snooze_time: metadata.snooze_time,
                subject: metadata.subject,
                time: metadata.time,
                to_list: MessageAddresses {
                    value: metadata.to_list.into_iter().map(|v| v.into()).collect(),
                },
                unread: metadata.unread,
                row_id: None,
                stash: Some(stash.clone()),
            };
            if let Some(existing) =
                Self::find_by_remote_id(message.remote_id.clone().unwrap(), stash).await?
            {
                message.local_id = existing.local_id;
                message.row_id = existing.row_id;
                message.stash = existing.stash;
            }
            message.save().await?;

            ids.push(message.local_id.unwrap());
        }
        Ok(ids)
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
    pub async fn delete_multiple<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        api.put_messages_delete(
            ids.into_iter().map(|id| id.into()).collect(),
            Some(label_id.into()),
        )
        .await
        .map(|r| r.responses)
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

    /// Extends [`Model::load()`] to pre-load child records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        self.attachments_metadata =
            Attachment::load_message_attachment_metadata(self.local_id.unwrap(), interface).await?;

        let labels = Label::find(
            r#"WHERE local_id IN (SELECT local_label_id FROM message_labels WHERE local_message_id = ?)"#,
            params![self.local_id],
            interface,
            None,
        )
        .await?;

        self.exclusive_location = ExclusiveLocation::from_labels(&labels);
        self.label_ids = labels.into_iter().map(|l| l.remote_id.unwrap()).collect();

        if let Some(body) = MessageBodyMetadata::find_first(
            "WHERE local_message_id = ?",
            params![self.local_id],
            interface,
        )
        .await?
        {
            self.header = body.header;
            self.mime_type = body.mime_type;
            self.parsed_headers = body.parsed_headers;
        }

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
            let local_ids = Attachment::save_from_message_metadata(self, interface).await?;
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
    ///
    /// # Errors
    ///
    /// Returns error if the message failed to download, the db query failed or
    /// the message body could not be written to the cache.
    ///
    pub async fn message_body<P: PgpProviderSync, PM: ProtonMail>(
        &self,
        cache: &ProtonCache<CacheMessageConfig>,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
    ) -> Result<DecryptedMessageBody, AppError> {
        let key = self.local_id.expect("Message does not have a local id");
        if let Some(mut content) = cache.get_item(&key)? {
            let mut body = String::new();
            content.read_to_string(&mut body)?;
            let metadata = self
                .get_message_body_metadata()
                .await?
                .ok_or(AppError::MessageBodyMetadataMissing(key))?;
            Ok(DecryptedMessageBody { body, metadata })
        } else {
            let decrypted_message_body = self
                .decrypt_from_remote(address_keys, pgp_provider, api)
                .await?;
            cache.add_item(key, decrypted_message_body.body.clone().as_bytes())?;
            Ok(decrypted_message_body)
        }
    }

    /// Get message from remote and decrypt it
    pub async fn decrypt_from_remote<P: PgpProviderSync, PM: ProtonMail>(
        &self,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
    ) -> Result<DecryptedMessageBody, AppError> {
        // Fetch metadata first to sync contents and cache.
        let encrypted_msg = self.sync_message_body(api).await?;

        // TODO: Verify signature.
        let (decrypted_body, _) = encrypted_msg
            .decrypt(&pgp_provider, &address_keys)
            .map_err(|e| {
                error!("Failed to decrypt message ({:?}): {e}", self.local_id);
                AppError::Other("e".to_owned())
            })?;

        match decrypted_body {
            DecryptedBody::Plain(body) => Ok(DecryptedMessageBody {
                metadata: encrypted_msg.metadata,
                body,
            }),
            DecryptedBody::Mime(multipart) => {
                // TODO(ET-263): Handle multipart messages.
                Ok(DecryptedMessageBody {
                    metadata: encrypted_msg.metadata,
                    body: multipart.body,
                })
            }
        }
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
    pub async fn search<PM: ProtonMail>(
        options: GetMessagesOptions,
        api: &PM,
        stash: &Stash,
    ) -> Result<Vec<Message>, AppError> {
        let ids = Self::create_or_update_messages_from_metadata(
            Self::fetch_metadata(options, api).await?.messages,
            stash,
        )
        .await?;
        let mut messages = vec![];
        for id in ids {
            messages.push(
                Self::load(id, stash)
                    .await?
                    .ok_or(AppError::Other("Message not found".to_owned()))?,
            );
        }
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
                page_size: count.max(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} messages TOTAL={}",
            response.messages.len(),
            response.total
        );

        Self::create_or_update_messages_from_metadata(response.messages, stash).await?;
        Ok(())
    }

    /// Synchronize the message body.
    ///
    /// # Parameters
    ///
    /// * `cache_path` - TODO: Document this parameter.
    /// * `api`        - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns error if the API request failed or the data could not be written
    /// to the database.
    ///
    pub async fn sync_message_body<PM: ProtonMail>(
        &self,
        api: &PM,
    ) -> Result<EncryptedMessageBody, AppError> {
        let (metadata, body) = self.sync_message_metadata(api).await?;
        let encrypted_body = if let Some(body) = body {
            body
        } else {
            self.get_message_from_remote(api).await?.body
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
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns error if the API request failed or the data could not be written
    /// to the database.
    ///
    async fn sync_message_metadata<PM: ProtonMail>(
        &self,
        api: &PM,
    ) -> Result<(MessageBodyMetadata, Option<String>), AppError> {
        let Some(conn) = self.stash() else {
            return Err(StashError::NoStashAvailable.into());
        };

        if let Some(metadata) = self.get_message_body_metadata().await? {
            Ok((metadata, None))
        } else {
            let message = self.get_message_from_remote(api).await?;

            // create message in the database and store body in the cache.
            let mut metadata = MessageBodyMetadata {
                local_message_id: message.local_id,
                remote_message_id: message.remote_id.clone(),
                header: message.header.clone(),
                parsed_headers: message.parsed_headers,
                mime_type: message.mime_type,
                row_id: None,
                stash: Some(conn.clone()),
            };
            metadata
                .save()
                .await
                .inspect_err(|e| error!("Failed to store message body metadata in db: {e}"))?;
            Ok((metadata, Some(message.body)))
        }
    }

    /// Get message body metadata from DB.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    async fn get_message_body_metadata(&self) -> Result<Option<MessageBodyMetadata>, AppError> {
        let Some(conn) = self.stash() else {
            return Err(StashError::NoStashAvailable.into());
        };

        Ok(MessageBodyMetadata::find_first(
            "WHERE local_message_id = ?",
            params![self.local_id],
            conn,
        )
        .await
        .inspect_err(|e| error!("Failed to retrieve message body metadata from db: {e}"))?)
    }

    /// Get message from remote
    async fn get_message_from_remote<PM: ProtonMail>(&self, api: &PM) -> Result<Message, AppError> {
        // metadata is not there it is either missing or the message does not exist.
        let remote_id = self.remote_id.clone().ok_or(AppError::Other(
            "MailboxError::MessageDoesNotHaveRemoteId(self.local_id)".to_owned(),
        ))?;
        // sync the message body
        Ok(Message::from(
            api.get_message(remote_id.into())
                .await
                .map(|v| v.message)
                .map_err(|e| {
                    error!("Failed to retrieve message: {e}");
                    ApiServiceError::UnknownError("MailContextError::from(e)".to_owned())
                })?,
        ))
    }
}

impl From<ApiMessage> for Message {
    fn from(value: ApiMessage) -> Self {
        let label_ids: Vec<LabelId> = value
            .metadata
            .label_ids
            .into_iter()
            .map(|v| v.into())
            .collect();

        Self {
            local_id: None,
            remote_id: Some(value.metadata.id.into()),
            local_conversation_id: None,
            remote_conversation_id: Some(value.metadata.conversation_id.into()),
            address_id: value.metadata.address_id.into(),
            attachments_metadata: value
                .metadata
                .attachments_metadata
                .into_iter()
                .map(|v| v.into())
                .collect(),
            bcc_list: MessageAddresses {
                value: value
                    .metadata
                    .bcc_list
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
            body: value.body,
            cc_list: MessageAddresses {
                value: value
                    .metadata
                    .cc_list
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
            deleted: false,
            display_order: value.metadata.order,
            expiration_time: value.metadata.expiration_time,
            external_id: value.metadata.external_id.map(|v| v.into()),
            header: value.header,
            flags: value.metadata.flags.into(),
            is_forwarded: value.metadata.is_forwarded,
            is_replied: value.metadata.is_replied,
            is_replied_all: value.metadata.is_replied_all,
            exclusive_location: None,
            label_ids,
            mime_type: value.mime_type.into(),
            num_attachments: value.metadata.num_attachments,
            parsed_headers: ParsedHeaders {
                headers: value.parsed_headers,
            },
            reply_tos: MessageAddresses {
                value: value
                    .metadata
                    .reply_tos
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
            sender: value.metadata.sender.into(),
            size: value.metadata.size,
            snooze_time: value.metadata.snooze_time,
            subject: value.metadata.subject,
            time: value.metadata.time,
            to_list: MessageAddresses {
                value: value
                    .metadata
                    .to_list
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
            unread: value.metadata.unread,
            row_id: None,
            stash: None,
        }
    }
}

#[cfg(test)]
mod default_message {
    use proton_core_common::datatypes::RemoteId;

    use crate::models::Message;

    impl Default for Message {
        fn default() -> Self {
            Self {
                address_id: RemoteId::new(Default::default()),
                // The rest are by default default.
                flags: Default::default(),
                local_id: Default::default(),
                remote_id: Default::default(),
                local_conversation_id: Default::default(),
                remote_conversation_id: Default::default(),
                attachments_metadata: Default::default(),
                bcc_list: Default::default(),
                body: Default::default(),
                cc_list: Default::default(),
                deleted: Default::default(),
                expiration_time: Default::default(),
                external_id: Default::default(),
                header: Default::default(),
                is_forwarded: Default::default(),
                is_replied: Default::default(),
                is_replied_all: Default::default(),
                label_ids: Default::default(),
                exclusive_location: Default::default(),
                mime_type: Default::default(),
                num_attachments: Default::default(),
                display_order: Default::default(),
                parsed_headers: Default::default(),
                reply_tos: Default::default(),
                sender: Default::default(),
                size: Default::default(),
                snooze_time: Default::default(),
                subject: Default::default(),
                time: Default::default(),
                to_list: Default::default(),
                unread: Default::default(),
                row_id: Default::default(),
                stash: Default::default(),
            }
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
pub struct MessageBodyMetadata {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_message_id: Option<u64>,

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
