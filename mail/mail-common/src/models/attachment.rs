use crate::datatypes::{
    attachment, AttachmentEncryptedSignature, AttachmentMetadata, AttachmentSignature, Disposition,
    KeyPackets, MessageSender,
};
use crate::models::*;
use crate::AppError;
use bytes::Bytes;
use indoc::indoc;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::AddressId;
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, MessageAttachment as ApiMessageAttachment,
};
use proton_api_mail::services::proton::responses::GetAttachmentMetadataResponse;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LocalAddressId, LocalId, RemoteId};
use proton_core_common::models::{Address, ModelIdExtension};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, DecryptableAttachment,
    KeyPackets as RealKeyPackets,
};
use serde::{Deserialize, Serialize};
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};

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
/// *NEVER* use [`Model::save()`]  but instead
/// *ALWAYS* use [`Attachment::save()`].
///
///
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
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
    pub local_address_id: Option<LocalAddressId>,

    /// Address with which this attachment was encrypted. The address id can
    /// only be retrieved from a [`Message`] or the full [`Attachment`] type.
    #[DbField]
    pub remote_address_id: Option<AddressId>,

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
    pub sender: Option<MessageSender>,

    /// TODO: Document this field.
    #[DbField]
    pub signature: Option<AttachmentSignature>,

    /// Size of the attachment in bytes.
    #[DbField]
    pub size: u64,

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
    #[serde(skip)]
    pub row_id: Option<u64>,
}

impl ModelIdExtension for Attachment {
    type RemoteId = RemoteId;
}

impl Attachment {
    /// Load attachment metadata for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn load_conversation_attachment_metadata(
        conversation_id: LocalId,
        tether: &Tether,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        Self::find("WHERE local_id IN (SELECT local_attachment_id FROM conversation_attachments WHERE local_conversation_id = ?)",
                params![conversation_id],
                   tether,
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
        tether: &Tether,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        Self::find("WHERE local_id IN (SELECT local_attachment_id FROM message_attachments WHERE local_message_id = ?)",
                   params![message_id],
                   tether,
        )
        .await.map(|v| v.into_iter().map(Into::into).collect())
    }

    /// Save or update the attachment in the database.
    ///
    /// It's imperative to call this function rather than
    /// [`Model::save()`] to make sure that we override the existing
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
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.local_id.is_none() {
            if let Some(remote_id) = self.remote_id.clone() {
                if let Some(existing) =
                    Self::find_first("WHERE remote_id=?", params![remote_id], bond).await?
                {
                    self.local_id = existing.local_id;
                    self.row_id = existing.row_id;
                }
            }
        }
        if self.local_address_id.is_none() {
            if let Some(remote_address_id) = self.remote_address_id.clone() {
                self.local_address_id =
                    Address::remote_id_counterpart(remote_address_id, bond).await?;
            }
        }

        if self.local_message_id.is_none() {
            if let Some(remote_message_id) = self.remote_message_id.clone() {
                self.local_message_id =
                    Message::remote_id_counterpart(remote_message_id, bond).await?;
            }
        }

        if self.local_conversation_id.is_none() {
            if let Some(remote_conversation_id) = self.remote_conversation_id.clone() {
                self.local_conversation_id =
                    Conversation::remote_id_counterpart(remote_conversation_id, bond).await?;
            }
        }

        <Self as Model>::save(self, bond).await
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
        api.get_attachment(id).await
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
        api.get_attachment_metadata(id).await
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
        tether: &mut Tether,
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
        let tx = tether.transaction().await?;
        attachment.save(&tx).await?;
        tx.commit().await?;
        *self = attachment;
        Ok(Some(()))
    }

    /// Get all attachments for a given message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn for_message(
        local_message_id: LocalId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Attachment::find(
            indoc! {"
            WHERE local_id IN (
                SELECT local_attachment_id FROM message_attachments
                WHERE local_message_id=?
            )
        "},
            params![local_message_id],
            tether,
        )
        .await
    }

    /// Get all attachments with the given IDs.
    ///
    /// # Parameters
    ///
    /// * `attachment_ids` - List of local attachment ids.
    /// * `interface` - The database interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn find_by_ids(
        attachment_ids: impl IntoIterator<Item = LocalId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let params: Vec<Box<dyn ToSql + Send>> = attachment_ids
            .into_iter()
            .map(|v| -> Box<dyn ToSql + Send> { Box::new(v) })
            .collect();
        Attachment::find(
            format!("WHERE local_id IN ({})", vec!["?"; params.len()].join(","),),
            params,
            tether,
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
            remote_id: Some(value.id),
            local_address_id: None,
            remote_address_id: Some(value.address_id),
            local_conversation_id: None,
            remote_conversation_id: Some(value.conversation_id),
            local_message_id: None,
            remote_message_id: Some(value.message_id),
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.clone().map(|v| v.into()),
            is_auto_forwardee: value.is_auto_forwardee,
            key_packets: Some(value.key_packets.clone().into()),
            mime_type: value.mime_type.parse().unwrap_or_default(),
            filename: value.name,
            sender: value.sender.map(|v| v.into()),
            signature: value.signature.map(|v| v.into()),
            size: value.size,
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
            row_id: None,
        }
    }
}

impl From<ApiMessageAttachment> for Attachment {
    fn from(value: ApiMessageAttachment) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            local_address_id: None,
            remote_address_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_message_id: None,
            remote_message_id: None,
            disposition: value.disposition.into(),
            enc_signature: value.enc_signature.clone().map(|v| v.into()),
            is_auto_forwardee: false,
            key_packets: Some(value.key_packets.clone().into()),
            mime_type: value.mime_type.parse().unwrap_or_default(),
            filename: value.name,
            sender: None,
            signature: value.signature.map(|v| v.into()),
            size: value.size,
            content_id: value.headers.content_id,
            transfer_encoding: value.headers.content_transfer_encoding,
            image_width: value.headers.image_width,
            image_height: value.headers.image_height,
            row_id: None,
        }
    }
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
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
            row_id: None,
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

#[cfg(test)]
#[path = "../tests/models/attachments.rs"]
mod attachments;
