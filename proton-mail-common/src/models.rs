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

use crate::datatypes::{
    AlmostAllMail, AttachmentEncryptedSignature, AttachmentMetadata, AttachmentMetadatas,
    AttachmentSignature, ComposerDirection, ComposerMode, ConversationCount, DecryptedMessageBody,
    Disposition, EncryptedMessageBody, KeyPackets, LabelIds, LabelType, MessageAddress,
    MessageAddresses, MessageAttachmentInfo, MessageAttachments, MessageButtons, MessageCount,
    MessageFlags, MimeType, MobileSettings, NextMessageOnMove, ParsedHeaders, PgpScheme,
    PmSignature, ShowImages, ShowMoved, SpamAction, SwipeAction, SystemLabelId, ViewLayout,
    ViewMode,
};
use crate::{AppError, ALL_LABEL_TYPES};
use bytes::Bytes;
use indoc::formatdoc;
use proton_api_core::service::ApiServiceError;
use proton_api_mail::services::proton::requests::{
    GetConversationsOptions, GetMessagesOptions, PostLabelsRequest, PutLabelRequest,
};
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, Conversation as ApiConversation,
    ConversationLabels as ApiConversationLabels, Label as ApiLabel,
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageMetadata as ApiMessageMetadata,
    OperationResult,
};
use proton_api_mail::services::proton::responses::{
    GetAttachmentMetadataResponse, GetMessagesResponse,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::MAX_PAGE_ELEMENT_COUNT;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_inbox::attachment::{
    AttachmentDecryption, AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync as PgpProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::UnlockedAddressKeys;
use smart_default::SmartDefault;
use stash::datatypes::QueryResultU64;
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, StashError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error};

pub const MAIL_SETTINGS_ID: u64 = 1;

/// TODO: Document this struct.
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

    /// TODO: Document this field.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub address_id: RemoteId,

    /// TODO: Document this field.
    #[DbField]
    pub conversation_id: RemoteId,

    /// TODO: Document this field.
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
    pub key_packets: KeyPackets,

    /// TODO: Document this field.
    #[DbField]
    pub message_id: RemoteId,

    /// TODO: Document this field.
    #[DbField]
    pub mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    /// TODO: Document this field.
    pub real_enc_signature: Option<RealAttachmentEncryptedSignature>,

    /// TODO: Document this field.
    pub real_key_packets: Option<RealKeyPackets>,

    /// TODO: Document this field.
    pub real_signature: Option<RealAttachmentSignature>,

    /// TODO: Document this field.
    #[DbField]
    pub sender: Option<MessageAddress>,

    /// TODO: Document this field.
    #[DbField]
    pub signature: Option<AttachmentSignature>,

    /// TODO: Document this field.
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

impl Attachment {
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
        !self.key_packets.to_string().is_empty()
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
        attachment.stash.clone_from(&self.stash);
        attachment.save().await?;
        *self = attachment;
        Ok(Some(()))
    }
}

// TODO: The use of the "Real" wrappers is because the source types don't
// TODO: implement the traits we need. At a later date we should implement those
// TODO: traits directly on the source types, and remove these wrappers.
impl AttachmentDecryption for Attachment {
    fn attachment_key_packets(&self) -> &RealKeyPackets {
        self.real_key_packets.as_ref().unwrap()
    }

    fn attachment_signature(&self) -> &Option<RealAttachmentSignature> {
        &self.real_signature
    }

    fn attachment_encrypted_signature(&self) -> &Option<RealAttachmentEncryptedSignature> {
        &self.real_enc_signature
    }
}

impl From<ApiAttachment> for Attachment {
    fn from(value: ApiAttachment) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            address_id: value.address_id.into(),
            conversation_id: value.conversation_id.into(),
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.clone().map(|v| v.into()),
            is_auto_forwardee: value.is_auto_forwardee,
            key_packets: value.key_packets.clone().into(),
            message_id: value.message_id.into(),
            mime_type: value.mime_type.into(),
            name: value.name,
            real_enc_signature: value.enc_signature,
            real_key_packets: Some(value.key_packets),
            real_signature: value.signature.clone(),
            sender: value.sender.map(|v| v.into()),
            signature: value.signature.map(|v| v.into()),
            size: value.size,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("conversations")]
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
    pub attachment_info: HashMap<String, MessageAttachmentInfo>,

    /// TODO: Document this field.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[DbField]
    pub display_snooze_reminder: bool,

    /// TODO: Document this field.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    pub labels: Vec<ConversationLabels>,

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
    pub order: u64,

    /// TODO: Document this field.
    pub recipients: Vec<MessageAddress>,

    /// TODO: Document this field.
    pub senders: Vec<MessageAddress>,

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
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn apply_label_to_multiple(
        label_id: u64,
        ids: Vec<u64>,
        stash: &Stash,
    ) -> Result<(), StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        for id in ids {
            // label all conversation messages
            stash
                .execute(
                    formatdoc!(
                        r"
                WITH
                    conv_msgs
                AS (
                    SELECT id, ? AS label_id FROM messages WHERE conversation_id = ?
                )
                INSERT OR IGNORE INTO
                    message_labels (message_id, label_id)
                SELECT
                    *
                FROM
                    conv_msgs
                RETURNING
                    message_id
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
        let tx = stash.transaction().await?;
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            if let Some(existing) = Self::find(
                "WHERE remote_id = ?",
                params![conv.remote_id.clone()],
                stash,
                None,
            )
            .await?
            .into_iter()
            .next()
            {
                conv.local_id = existing.local_id;
                conv.row_id = existing.row_id;
                conv.stash = existing.stash;

                // Remove any labels that are no longer associated with this conversation.
                if !conv.labels.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            conversation_labels
                        WHERE
                            local_conversation_id = ?
                            AND local_label_id NOT IN (
                                SELECT local_id FROM labels WHERE remote_id IN ({})
                            )
                        ",
                            vec!["?"; conv.labels.len()].join(",")
                        ),
                        vec![Box::new(conv.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>]
                            .into_iter()
                            .chain(conv.labels.iter().map(|label| {
                                Box::new(label.remote_id.clone()) as Box<dyn ToSql + Send>
                            }))
                            .collect(),
                    )
                    .await?;
                } else {
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            conversation_labels
                        WHERE
                            local_conversation_id = ?
                        ",
                        ),
                        params![conv.local_id],
                    )
                    .await?;
                }

                // Remove any attachments that are no longer associated with this conversation.
                if !conv.attachments_metadata.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            conversation_attachments
                        WHERE
                            local_conversation_id = ?
                            AND local_attachment_id NOT IN ({})
                        ",
                            vec!["?"; conv.attachments_metadata.len()].join(",")
                        ),
                        vec![Box::new(conv.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>]
                            .into_iter()
                            .chain(conv.attachments_metadata.iter().map(|attachment| {
                                Box::new(attachment.remote_id.clone()) as Box<dyn ToSql + Send>
                            }))
                            .collect(),
                    )
                    .await?;
                } else {
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            conversation_attachments
                        WHERE
                            local_conversation_id = ?
                        ",
                        ),
                        params![conv.local_id],
                    )
                    .await?;
                }
            }
            conv.save_using(&tx).await?;

            for mut label in conv.labels {
                label.save_using(&tx).await?;
            }
            for mut _attachment in conv.attachments_metadata {
                // TODO
                // attachment.save_using(&tx).await?;
                continue;
            }

            ids.push(conv.local_id.unwrap());
        }
        tx.commit().await?;
        Ok(ids)
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
                    INSERT INTO
                        label_conversation_count
                    VALUES
                        (SELECT id FROM labels WHERE rid = ?), ?, ?)
                        ON CONFLICT
                            (label_id)
                        DO UPDATE SET
                            total = excluded.total,
                            unread = excluded.unread
                    "
                ),
                params![count.label_id, count.total, count.unread],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn delete_multiple(
        ids: Vec<u64>,
        label_id: u64,
        stash: &Stash,
    ) -> Result<usize, StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        stash
            .execute(
                formatdoc!(
                    r"
            UPDATE
                messages
            SET
                deleted = 1
            WHERE
                conversation_id IN ({})
                AND deleted = 0
                AND id IN (
                    SELECT message_id FROM message_labels WHERE label_id = ?
                )
            RETURNING
                id
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
    /// * `stash`      - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_local_ids(
        remote_ids: Vec<RemoteId>,
        stash: &Stash,
    ) -> Result<Vec<u64>, StashError> {
        let mut ids = Vec::new();
        for remote_id in remote_ids {
            if let Some(conv) = Self::find(
                "WHERE remote_id = ?",
                params![remote_id.clone()],
                stash,
                None,
            )
            .await?
            .first()
            {
                if let Some(local_id) = conv.local_id {
                    ids.push(local_id);
                }
            }
        }
        Ok(ids)
    }

    /// Find remote IDs for the given local IDs.
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The local IDs to find remote IDs for.
    /// * `stash`     - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn find_remote_ids(
        local_ids: Vec<u64>,
        stash: &Stash,
    ) -> Result<Vec<RemoteId>, StashError> {
        let mut ids = Vec::new();
        for local_id in local_ids {
            if let Some(conv) = Self::load(local_id, stash).await? {
                if let Some(remote_id) = conv.remote_id {
                    ids.push(remote_id);
                }
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
    pub fn first_unread_message(label: &Label, messages: &[Message]) -> Option<RemoteId> {
        if messages.is_empty() {
            return None;
        }

        if label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id == Some(LabelId::starred())
        {
            // last consecutive that is not a draft
            let mut last_unread = None;

            for msg in messages.iter().rev() {
                if msg.unread && !msg.flags.is_draft() {
                    last_unread.clone_from(&msg.remote_id);
                } else if last_unread.is_some() {
                    break;
                }
            }

            return last_unread;
        };

        // In any other location check if the last message is unread.
        let mut iter = messages.iter().rev();
        let msg = iter.next()?;
        if msg.unread && !(msg.flags.is_draft() || msg.flags.is_sent_auto()) {
            return msg.remote_id.clone();
        }

        let mut last_unread = None;

        // last consecutive message that is not a draft or sent auto-reply
        for msg in iter {
            if msg.unread && !(msg.flags.is_draft() || msg.flags.is_sent_auto()) {
                last_unread.clone_from(&msg.remote_id);
            } else if last_unread.is_some() {
                break;
            }
        }

        last_unread
    }

    /// TODO: Document this method.
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.labels
            .iter()
            .any(|l| l.remote_id == Some(LabelId::starred()))
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as read.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_multiple_as_read(ids: Vec<u64>, stash: &Stash) -> Result<(), StashError> {
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
    /// * `ids`   - The IDs of the conversations to mark as unread.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_multiple_as_unread(ids: Vec<u64>, stash: &Stash) -> Result<(), StashError> {
        // TODO: This is simplified, and will be updated when these operations are
        // TODO: refactored
        for id in ids {
            if let Some(mut conv) = Conversation::load(id, stash).await? {
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
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn remove_label_from_multiple(
        label_id: u64,
        ids: Vec<u64>,
        stash: &Stash,
    ) -> Result<(), StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        for id in ids {
            // label all conversation messages
            stash
                .execute(
                    formatdoc!(
                        r"
                WITH
                    conv_msgs
                AS (
                    SELECT id, unread FROM messages WHERE conversation_id = ?1
                )
                DELETE FROM
                    message_labels
                WHERE
                    message_id IN (
                        SELECT id FROM messages WHERE conversation_id = ?1
                    )
                    AND message_labels.label_id = ?2
                RETURNING
                    message_id
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
        Conversation::create_or_update_conversation_counts(conversation_counts, tx.stash()).await?;
        Message::create_or_update_message_counts(message_counts, tx.stash()).await?;
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
    /// * `stash`    - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn undelete_multiple(
        ids: Vec<u64>,
        label_id: u64,
        stash: &Stash,
    ) -> Result<usize, StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        stash
            .execute(
                formatdoc!(
                    r"
            UPDATE
                messages
            SET
                deleted = 0
            WHERE
                conversation_id IN ({})
                AND deleted = 1
                AND id IN (
                    SELECT message_id FROM message_labels WHERE label_id = ?
                )
            RETURNING id",
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
}

impl From<ApiConversation> for Conversation {
    fn from(value: ApiConversation) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            attachment_info: value
                .attachment_info
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(|v| v.into())
                .collect(),
            display_snooze_reminder: value.display_snooze_reminder,
            expiration_time: value.expiration_time,
            labels: value.labels.into_iter().map(|v| v.into()).collect(),
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            order: value.order,
            recipients: value.recipients.into_iter().map(|v| v.into()).collect(),
            senders: value.senders.into_iter().map(|v| v.into()).collect(),
            size: value.size,
            subject: value.subject,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("conversation_labels")]
pub struct ConversationLabels {
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

impl From<ApiConversationLabels> for ConversationLabels {
    fn from(value: ApiConversationLabels) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
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
    pub parent_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub color: String,

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
    pub order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub path: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub sticky: bool,

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

    pub fn is_applicable_label(&self) -> bool {
        self.label_type == LabelType::Label
            || self
                .remote_id
                .as_ref()
                .map_or(false, |rid| *rid == LabelId::starred())
    }

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
        let tx = stash.transaction().await?;
        for label in all_labels.iter_mut() {
            label.save_using(&tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Get the total count of associated conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn total_conversations(&self) -> Result<u64, StashError> {
        let stash = self.stash.as_ref().ok_or(StashError::NoStashAvailable)?;
        let id = Some(self.local_id).ok_or(StashError::IdNotSet)?;
        Ok(stash
            .query::<_, QueryResultU64>(
                formatdoc!(
                    r"
                    SELECT
                        total AS value
                    FROM
                        label_conversation_count
                    WHERE
                        local_label_id = ?
                    ",
                ),
                params![id],
            )
            .await?
            .first()
            .map(|r| r.value)
            .unwrap_or(0))
    }

    /// Get the total count of associated messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn total_messages(&self) -> Result<u64, StashError> {
        let stash = self.stash.as_ref().ok_or(StashError::NoStashAvailable)?;
        let id = Some(self.local_id).ok_or(StashError::IdNotSet)?;
        Ok(stash
            .query::<_, QueryResultU64>(
                formatdoc!(
                    r"
                    SELECT
                        total AS value
                    FROM
                        label_message_count
                    WHERE
                        local_label_id = ?
                    ",
                ),
                params![id],
            )
            .await?
            .first()
            .map(|r| r.value)
            .unwrap_or(0))
    }

    /// Get the count of associated unread conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn unread_conversations(&self) -> Result<u64, StashError> {
        let stash = self.stash.as_ref().ok_or(StashError::NoStashAvailable)?;
        let id = Some(self.local_id).ok_or(StashError::IdNotSet)?;
        Ok(stash
            .query::<_, QueryResultU64>(
                formatdoc!(
                    r"
                    SELECT
                        unread AS value
                    FROM
                        label_conversation_count
                    WHERE
                        local_label_id = ?
                    ",
                ),
                params![id],
            )
            .await?
            .first()
            .map(|r| r.value)
            .unwrap_or(0))
    }

    /// Get the count of associated unread messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    ///
    pub async fn unread_messages(&self) -> Result<u64, StashError> {
        let stash = self.stash.as_ref().ok_or(StashError::NoStashAvailable)?;
        let id = Some(self.local_id).ok_or(StashError::IdNotSet)?;
        Ok(stash
            .query::<_, QueryResultU64>(
                formatdoc!(
                    r"
                    SELECT
                        unread AS value
                    FROM
                        label_message_count
                    WHERE
                        local_label_id = ?
                    ",
                ),
                params![id],
            )
            .await?
            .first()
            .map(|r| r.value)
            .unwrap_or(0))
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

    /// Return the preferred view mode for this label.
    ///
    /// If this function returns [`None`] we should use the
    /// [`ViewMode`] defined in the user's [`MailSettings`],
    /// otherwise the returned value should be used.
    ///
    pub fn view_mode(&self) -> Option<ViewMode> {
        let remote_id = self.remote_id.as_ref()?;

        if *remote_id == LabelId::drafts()
            || *remote_id == LabelId::sent()
            || *remote_id == LabelId::all_drafts()
            || *remote_id == LabelId::all_sent()
        {
            return Some(ViewMode::Messages);
        }

        None
    }
}

impl From<ApiLabel> for Label {
    fn from(value: ApiLabel) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            parent_id: value.parent_id.map(|id| id.into()),
            color: value.color,
            display: value.display,
            expanded: value.expanded,
            initialized_conv: false,
            initialized_msg: false,
            label_type: value.label_type.into(),
            name: value.name,
            notify: value.notify,
            order: value.order,
            path: None,
            sticky: false,
            row_id: None,
            stash: None,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq, SmartDefault)]
#[allow(clippy::struct_excessive_bools)]
#[TableName("settings")]
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
    pub async fn sync_mail_settings<PM: ProtonMail>(api: &PM) -> Result<(), AppError> {
        let mut settings = MailSettings::from(api.get_settings().await.map(|r| r.mail_settings)?);
        debug!("Storing labels into database");
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
    pub remote_conversation_id: RemoteId,

    /// TODO: Document this field.
    #[DbField]
    pub address_id: RemoteId,

    /// TODO: Document this field.
    #[DbField]
    pub attachments: MessageAttachments,

    /// TODO: Document this field.
    #[DbField]
    pub attachments_metadata: AttachmentMetadatas,

    /// TODO: Document this field.
    #[DbField]
    pub bcc_list: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub body: String,

    /// TODO: Document this field.
    #[DbField]
    pub cc_list: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub external_id: Option<ExternalId>,

    /// TODO: Document this field.
    #[DbField]
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
    #[DbField]
    pub label_ids: LabelIds,

    /// TODO: Document this field.
    #[DbField]
    pub mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub num_attachments: u32,

    /// TODO: Document this field.
    #[DbField]
    pub order: u64,

    /// TODO: Document this field.
    #[DbField]
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
        let tx = stash.transaction().await?;
        let mut ids = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            let mut message = Self {
                local_id: None,
                remote_id: Some(metadata.id.into()),
                address_id: metadata.address_id.into(),
                attachments: MessageAttachments { value: Vec::new() },
                attachments_metadata: AttachmentMetadatas {
                    value: metadata
                        .attachments_metadata
                        .into_iter()
                        .map(|v| v.into())
                        .collect(),
                },
                bcc_list: MessageAddresses {
                    value: metadata.bcc_list.into_iter().map(|v| v.into()).collect(),
                },
                body: "".to_owned(),
                cc_list: MessageAddresses {
                    value: metadata.cc_list.into_iter().map(|v| v.into()).collect(),
                },
                expiration_time: metadata.expiration_time,
                external_id: metadata.external_id.map(|v| v.into()),
                flags: metadata.flags.into(),
                header: "".to_owned(),
                is_forwarded: metadata.is_forwarded,
                is_replied: metadata.is_replied,
                is_replied_all: metadata.is_replied_all,
                label_ids: LabelIds {
                    value: metadata.label_ids.into_iter().map(|v| v.into()).collect(),
                },
                local_conversation_id: None,
                mime_type: MimeType::TextPlain,
                num_attachments: metadata.num_attachments,
                order: metadata.order,
                parsed_headers: ParsedHeaders {
                    headers: HashMap::new(),
                },
                remote_conversation_id: metadata.conversation_id.into(),
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
            if let Some(existing) = Self::find(
                "WHERE remote_id = ?",
                params![message.remote_id.clone()],
                stash,
                None,
            )
            .await?
            .into_iter()
            .next()
            {
                message.local_id = existing.local_id;
                message.row_id = existing.row_id;
                message.stash = existing.stash;

                // Remove any labels that are no longer associated with this conversation.
                if !message.label_ids.value.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(
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
                            vec!["?"; message.label_ids.value.len()].join(",")
                        ),
                        vec![Box::new(message.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>]
                            .into_iter()
                            .chain(
                                message
                                    .label_ids
                                    .value
                                    .iter()
                                    .map(|label| Box::new(label.clone()) as Box<dyn ToSql + Send>),
                            )
                            .collect(),
                    )
                    .await?;
                } else {
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            message_labels
                        WHERE
                            local_message_id = ?
                        ",
                        ),
                        params![message.local_id],
                    )
                    .await?;
                }

                // Remove any attachments that are no longer associated with this conversation.
                if !message.attachments_metadata.value.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            message_attachments
                        WHERE
                            local_message_id = ?
                            AND local_attachment_id NOT IN ({})
                        ",
                            vec!["?"; message.attachments_metadata.value.len()].join(",")
                        ),
                        vec![Box::new(message.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>]
                            .into_iter()
                            .chain(message.attachments_metadata.value.iter().map(|attachment| {
                                Box::new(attachment.remote_id.clone()) as Box<dyn ToSql + Send>
                            }))
                            .collect(),
                    )
                    .await?;
                } else {
                    tx.execute(
                        formatdoc!(
                            "
                        DELETE FROM
                            message_attachments
                        WHERE
                            local_message_id = ?
                        ",
                        ),
                        params![message.local_id],
                    )
                    .await?;
                }
            }
            message.save_using(&tx).await?;

            for mut _label in message.label_ids.value {
                // TODO
                // label.save_using(&tx).await?;
                continue;
            }
            for mut _attachment in message.attachments_metadata.value {
                // TODO
                // attachment.save_using(&tx).await?;
                continue;
            }

            ids.push(message.local_id.unwrap());
        }
        tx.commit().await?;
        Ok(ids)
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
                    INSERT INTO
                        label_message_count
                    VALUES
                        (SELECT id FROM labels WHERE rid = ?), ?, ?)
                        ON CONFLICT
                            (label_id)
                        DO UPDATE SET
                            total = excluded.total,
                            unread = excluded.unread
                    "
                ),
                params![count.label_id, count.total, count.unread],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
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
        cache_path: &Path,
        address_keys: UnlockedAddressKeys<P>,
        pgp_provider: P,
        api: &PM,
    ) -> Result<DecryptedMessageBody, AppError>
    where
        UnlockedAddressKeys<P>: AsRef<P::PrivateKey>,
    {
        // Fetch metadata first to sync contents and cache.
        let metadata = self.sync_message_body(cache_path, api).await?;

        // TODO(ET-231): Read body from cache.
        let encrypted_body =
            std::fs::read_to_string(self.message_cache_path(cache_path)).map_err(|e| {
                error!("Failed to read encrypted message body from cache: {e}");
                AppError::Other(
                    r#"MailboxError::Context(MailContextError::Other(anyhow!("{e}")))"#.to_owned(),
                )
            })?;

        // Decrypt message.

        let encrypted_msg = EncryptedMessageBody {
            metadata,
            encrypted_body,
        };

        //TODO: Verify signature.
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
                //TODO(ET-263): Handle multipart messages.
                Ok(DecryptedMessageBody {
                    metadata: encrypted_msg.metadata,
                    body: multipart.body,
                })
            }
        }
    }

    /// Get the cache path for a message body with `id`.
    ///
    /// # Parameters
    ///
    /// * `cache_path` - TODO: Document this parameter.
    ///
    pub fn message_cache_path(&self, cache_path: &Path) -> PathBuf {
        cache_path.join(format!(
            "message_body_{}",
            self.local_id.expect("Message does not have a local id")
        ))
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
        cache_path: &Path,
        api: &PM,
    ) -> Result<MessageBodyMetadata, AppError> {
        let Some(conn) = self.stash() else {
            return Err(StashError::NoStashAvailable.into());
        };
        // TODO(ET-231): Use caching solution.
        let metadata = if let Some(metadata) =
            MessageBodyMetadata::find("WHERE id = ?", params![self.local_id], conn, None)
                .await
                .map_err(|e| {
                    error!("Failed to retrieve message body metadata from db: {e}");
                    e
                })?
                .into_iter()
                .next()
        {
            metadata
        } else {
            // metadata is not there it is either missing or the message does not exist.
            let remote_id = self.remote_id.clone().ok_or(AppError::Other(
                "MailboxError::MessageDoesNotHaveRemoteId(self.local_id)".to_owned(),
            ))?;
            // sync the message body
            let message = Message::from(
                api.get_message(remote_id.into())
                    .await
                    .map(|v| v.message)
                    .map_err(|e| {
                        error!("Failed to retrieve message: {e}");
                        ApiServiceError::UnknownError("MailContextError::from(e)".to_owned())
                    })?,
            );

            // create message in the database and store body in the cache.
            let mut metadata = MessageBodyMetadata {
                local_message_id: message.local_id,
                remote_id: message.remote_id.clone(),
                header: message.header.clone(),
                parsed_headers: message.parsed_headers,
                mime_type: message.mime_type,
                row_id: None,
                stash: Some(conn.clone()),
            };
            metadata.save().await.map_err(|e| {
                error!("Failed to store message body metadata in db: {e}");
                e
            })?;

            // TODO(ET-231): Write to cache.
            std::fs::write(self.message_cache_path(cache_path), &message.body).map_err(|e| {
                error!("Failed to write message body: {e}");
                AppError::Other(
                    r#"MailboxError::Context(MailContextError::Other(anyhow!("{e}")))"#.to_owned(),
                )
            })?;

            metadata
        };

        Ok(metadata)
    }
}

impl From<ApiMessage> for Message {
    fn from(value: ApiMessage) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.metadata.id.into()),
            local_conversation_id: None,
            remote_conversation_id: value.metadata.conversation_id.into(),
            address_id: value.metadata.address_id.into(),
            attachments: MessageAttachments {
                value: value.attachments.into_iter().map(|v| v.into()).collect(),
            },
            attachments_metadata: AttachmentMetadatas {
                value: value
                    .metadata
                    .attachments_metadata
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
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
            expiration_time: value.metadata.expiration_time,
            external_id: value.metadata.external_id.map(|v| v.into()),
            header: value.header,
            flags: value.metadata.flags.into(),
            is_forwarded: value.metadata.is_forwarded,
            is_replied: value.metadata.is_replied,
            is_replied_all: value.metadata.is_replied_all,
            label_ids: LabelIds {
                value: value
                    .metadata
                    .label_ids
                    .into_iter()
                    .map(|v| v.into())
                    .collect(),
            },
            mime_type: value.mime_type.into(),
            num_attachments: value.metadata.num_attachments,
            order: value.metadata.order,
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
#[derive(Clone, Debug, Eq, Model, PartialEq)]
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
    pub remote_id: Option<RemoteId>,

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
