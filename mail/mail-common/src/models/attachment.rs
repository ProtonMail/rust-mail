use crate::datatypes::LocalConversationId;
use crate::datatypes::attachment::ContentId;
use crate::datatypes::attachment::MimeType;
use crate::datatypes::{
    AttachmentEncryptedSignature, AttachmentMetadata, AttachmentSignature, Disposition, KeyPackets,
    LocalAttachmentId, LocalMessageId, MessageSender, attachment,
};
use crate::models::*;
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use anyhow::{Context as _, anyhow};
use bytes::Bytes;
use indoc::{formatdoc, indoc};
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_core_common::datatypes::LocalAddressId;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_core_common::utils::MapVec as _;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, DecryptableAttachment, EncryptableAttachment,
    EncryptedAttachment, KeyPackets as RealKeyPackets,
};
use proton_crypto_inbox::proton_crypto::crypto::OpenPGPFingerprint;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    Attachment as ApiAttachment, MessageAttachment as ApiMessageAttachment,
};
use proton_mail_api::services::proton::responses::GetAttachmentMetadataResponse;
use serde::{Deserialize, Serialize};
use stash::exports::Connection;
use stash::exports::Transaction;
use stash::exports::{SqliteError, ToSql};
use stash::macros::Model;
use stash::orm::Model;
use stash::orm::ModelHooks;
use stash::stash::{Bond, StashError, Tether};
use stash::{params, sql_using_serde};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, error, info, trace};

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
///    1.1. The metadata is stored using [`Conversation::on_save()`]
///    method which ensures that it does not override a fully synchronized
///    [`Attachment`] and only updates the conversation local and remote id.
///    1.2. If no record for this attachment exists one is created.
/// 2. The user now opens the conversation, which sync the respective
///    [`Message`]s.
/// 3. [`Messages`] also contains [`AttachmentMetadata`] as well as the address
///    id for the key this attachment was encrypted with.
///    3.1 This is now stored with [`Message::on_save()`], which also
///    ensures it does not override a fully synced attachment and updates
///    the message ids and the address id.
///    3.2 If no attachment record exists, one is created.
/// 4. From 1 or 2, we can receive a request to fetch the full attachment.
///    At this stage we either have partial data from [`AttachmentMetadata`] or
///    a fully synchronized attachment.
///    4.1. We check witch is situation we are in with
///    [`has_complete_metadata()`].
///    4.2. If this returns false we need to sync the full attachment with
///    [`sync_complete_metadata()`].
///    4.3. If the check returns true, the attachment is ready for use.
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
#[derive(Clone, Debug, Eq, Model, PartialEq, Default)]
#[TableName("attachments")]
#[ModelHooks]
pub struct Attachment {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalAttachmentId>,

    #[DbField]
    pub attachment_type: AttachmentType,

    #[DbField]
    pub local_address_id: Option<LocalAddressId>,

    #[DbField]
    pub remote_address_id: Option<AddressId>,

    #[DbField]
    pub local_conversation_id: Option<LocalConversationId>,

    #[DbField]
    pub remote_conversation_id: Option<ConversationId>,

    #[DbField]
    pub local_message_id: Option<LocalMessageId>,

    #[DbField]
    pub remote_message_id: Option<MessageId>,

    #[DbField]
    pub disposition: Disposition,

    #[DbField]
    pub enc_signature: Option<AttachmentEncryptedSignature>,

    #[DbField]
    pub is_auto_forwardee: bool,

    #[DbField]
    pub key_packets: Option<KeyPackets>,

    #[DbField]
    pub mime_type: attachment::MimeType,

    #[DbField]
    pub filename: String,

    #[DbField]
    pub sender: Option<MessageSender>,

    #[DbField]
    pub signature: Option<AttachmentSignature>,

    /// Size of the attachment in bytes.
    #[DbField]
    pub size: u64,

    /// Content id of the attachment if inlined in the message.
    #[DbField]
    pub content_id: Option<ContentId>,

    #[DbField]
    pub transfer_encoding: Option<String>,

    #[DbField]
    pub image_width: Option<String>,

    #[DbField]
    pub image_height: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AttachmentType {
    /// Some if it exists in the server and None if it's local
    Remote(Option<AttachmentId>),
    Pgp,
}

impl AttachmentType {
    pub fn is_pgp(&self) -> bool {
        matches!(self, Self::Pgp)
    }

    pub fn to_json(&self) -> Result<String, StashError> {
        serde_json::to_string(self)
            .context("error serializing attachment_type")
            .map_err(StashError::Custom)
    }
}

impl Default for AttachmentType {
    fn default() -> Self {
        Self::Remote(None)
    }
}

sql_using_serde!(AttachmentType);

impl Attachment {
    pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 100;
    pub const MAX_ATTACHMENT_SIZE: u64 = 25 * 1024 * 1024;

    pub fn remote_id(&self) -> Option<AttachmentId> {
        match &self.attachment_type {
            AttachmentType::Remote(id) => id.clone(),
            _ => None,
        }
    }

    /// Load attachment metadata for a given `conversation_id`.
    ///
    /// Only attachments with [`Disposition::Attachment`] are loaded. For the full attachment
    /// list we need to get the message body.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub fn load_conversation_attachment_metadata(
        conversation_id: LocalConversationId,
        conn: &Connection,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        Ok(Self::find_sync("WHERE local_id IN (SELECT local_attachment_id FROM conversation_attachments WHERE local_conversation_id = ?) AND disposition = ?",
            (conversation_id, Disposition::Attachment),
                   conn,
        )
            ?.map_vec())
    }

    /// Load attachment metadata for a given `message_id`.
    ///
    /// Only attachments with [`Disposition::Attachment`] are loaded. For the full attachment
    /// list we need to get the message body.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub fn load_message_attachment_metadata(
        message_id: LocalMessageId,
        conn: &Connection,
    ) -> Result<Vec<AttachmentMetadata>, StashError> {
        let res = Self::find_sync(
            "WHERE local_id IN (SELECT local_attachment_id FROM message_attachments WHERE local_message_id = ?) AND disposition = ?",
            (message_id, Disposition::Attachment),
            conn,
        )?;
        Ok(res.map_vec())
    }

    /// Fetch attachment content from the API.
    ///
    /// Calls the API to load encrypted attachment content for the given
    /// attachment.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Attachment).
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_content<PM: ProtonMail>(
        id: AttachmentId,
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
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_metadata<PM: ProtonMail>(
        id: AttachmentId,
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
        let remote_id = self
            .remote_id()
            .ok_or_else(|| AppError::AttachmentHasNoRemoteId(self.id()))?;
        tracing::info!("Syncing attachment metadata for {remote_id:?}");
        let mut attachment = Self::from(Self::fetch_metadata(remote_id, api).await?.attachment);
        attachment.local_id = self.local_id;
        tether.tx(async |tx| attachment.save(tx).await).await?;
        *self = attachment;
        Ok(Some(()))
    }

    /// Get all attachments for a given message with `local_message_id`.
    ///
    /// These also include attachments that are pgp embedded and do not appear
    /// in the metadata list.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn for_message(
        local_message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        tether
            .sync_query(move |conn| Self::for_message_sync(local_message_id, conn))
            .await
    }

    pub fn for_message_sync(
        local_message_id: LocalMessageId,
        conn: &Connection,
    ) -> Result<Vec<Self>, StashError> {
        Attachment::find_sync(
            indoc! {"
            WHERE local_id IN (
                SELECT local_attachment_id FROM message_attachments
                WHERE local_message_id=?1
            )
        "},
            (local_message_id,),
            conn,
        )
    }

    /// Create or update the attachment table with partial information contained in
    /// [`AttachmentMetadata`] from a [`Message`].
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub fn create_or_update_from_message_metadata(
        message: &mut Message,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalAttachmentId>, StashError> {
        let mut result = Vec::with_capacity(message.attachments_metadata.len());
        let message_id = message.id();
        for metadata in &mut message.attachments_metadata {
            // Handle case where we have local not uploaded attachments.
            let maybe_existing_attachment = if let Some(local_id) = metadata.local_id {
                Attachment::load_by_id_sync(local_id, tx)?
            } else {
                Attachment::find_by_remote_id_sync(&metadata.attachment_type, tx)?
            };

            let id = if let Some(attachment) = maybe_existing_attachment {
                // This attachment exists, we need to update only the parts we
                // want to modify.
                tx.execute(
                    "UPDATE attachments SET
                    local_address_id = ?,
                    remote_address_id = ?,
                    local_message_id = ?,
                    remote_message_id = ?
                    WHERE local_id = ?
                ",
                    (
                        message.local_address_id,
                        message.remote_address_id.clone(),
                        message_id,
                        message.remote_id.clone(),
                        attachment.id(),
                    ),
                )
                .inspect_err(|e| error!("Failed to update attachment from message: {}", e))?;
                attachment.id()
            } else {
                let mut attachment = Attachment::from(metadata.clone());
                // This attachment does not exist, we need to create it.
                attachment.local_address_id = Some(message.local_address_id);
                attachment.remote_address_id = Some(message.remote_address_id.clone());
                attachment.local_message_id = message.local_id;
                attachment.remote_message_id = message.remote_id.clone();
                attachment
                    .save_sync(tx)
                    .inspect_err(|e| error!("Failed to save attachment from message: {e:?}"))?;
                attachment.id()
            };
            metadata.local_id = Some(id);
            result.push(id);
        }
        Ok(result)
    }

    /// Create or update the attachment table with partial information contained in
    /// [`AttachmentMetadata`] from a [`Conversation`].
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub fn create_or_update_from_conversation_metadata(
        conversation: &mut Conversation,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalAttachmentId>, StashError> {
        let conversation_id = conversation.id();
        let mut result = Vec::with_capacity(conversation.attachments_metadata.len());
        for metadata in &mut conversation.attachments_metadata {
            // Handle case where we have local not uploaded attachments.
            let maybe_existing_attachment = if let Some(local_id) = metadata.local_id {
                Attachment::load_by_id_sync(local_id, tx)?
            } else {
                Attachment::find_by_remote_id_sync(&metadata.attachment_type, tx)?
            };

            let id = if let Some(attachment) = maybe_existing_attachment {
                // This attachment exists, we need to update only the parts we
                // want to modify.
                tx.execute(
                    "UPDATE attachments SET
                    local_conversation_id = ?,
                    remote_conversation_id = ?
                    WHERE local_id = ?
                ",
                    (
                        conversation_id,
                        conversation.remote_id.clone(),
                        attachment.id(),
                    ),
                )
                .inspect_err(|e| error!("Failed to update attachment from conversation: {}", e))?;
                attachment.id()
            } else {
                let mut attachment = Attachment::from(metadata.clone());
                // This attachment does not exist, we need to create it.
                attachment.local_conversation_id = Some(conversation_id);
                attachment.remote_conversation_id = conversation.remote_id.clone();
                attachment.save_sync(tx).inspect_err(|e| {
                    error!("Failed to save attachment from conversation: {e:?}")
                })?;
                attachment.id()
            };
            metadata.local_id = Some(id);
            result.push(id);
        }

        Ok(result)
    }

    /// Get all attachments with the given IDs.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn find_by_ids(
        attachment_ids: impl IntoIterator<Item = LocalAttachmentId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let params = attachment_ids
            .into_iter()
            .map(|v| Box::new(v) as Box<dyn ToSql + Send>)
            .collect_vec();
        Attachment::find(
            format!(
                "WHERE local_id IN ({})",
                stash::utils::placeholders_n(params.len())
            ),
            params,
            tether,
        )
        .await
    }

    /// Encrypt an attachment `data` with the given `address_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the encryption failed or the address can't be located.
    pub async fn encrypt(
        context: &MailUserContext,
        address_id: &AddressId,
        data: impl AsRef<[u8]>,
    ) -> MailContextResult<EncryptedAttachment> {
        struct AttachmentData<'a>(&'a [u8]);

        impl EncryptableAttachment for AttachmentData<'_> {
            fn attachment_data(&self) -> &[u8] {
                self.0
            }
        }

        let encryptable_attachment = AttachmentData(data.as_ref());
        let pgp = new_pgp_provider();
        let tether = context.user_stash().connection().await?;

        let unlocked_address_keys = context
            .unlocked_address_keys(&pgp, &tether, address_id)
            .await?;

        drop(tether);

        let primary_address_key = unlocked_address_keys.primary_for_mail().map_err(|e| {
            error!("Could not retrieve primary address key: {e:?}");
            MailContextError::Crypto
        })?;

        encryptable_attachment
            .attachment_encrypt_and_sign(&pgp, &primary_address_key)
            .map_err(|e| {
                error!("Failed to encrypt attachment: {e:?}");
                MailContextError::Crypto
            })
    }

    #[tracing::instrument(skip(ctx, bond, att))]
    pub async fn create(
        ctx: &MailUserContext,
        bond: &Bond<'_>,
        att: EncryptedAttachment,
        filename: &str,
        mime_type: attachment::MimeType,
    ) -> Result<Self, MailContextError> {
        info!("Creating attachment");

        let mut this = Self {
            attachment_type: AttachmentType::Remote(None),
            key_packets: Some(RealKeyPackets::new_from_bytes(&att.metadata.key_packets).into()),
            filename: filename.into(),
            mime_type,
            ..Attachment::default()
        };

        this.save(bond).await?;

        Self::store_in_cache(ctx, &this.filename, this.id(), att.data, bond).await?;

        Ok(this)
    }

    /// Create a new attachment from the given file `path`.
    ///
    /// It is expected that the attachment data temporarily exists in another location before it
    /// will moved or copied  to the internal cache.
    ///
    /// By default, the file name of the attachment will be the file name component of the specified
    /// `path`.
    #[tracing::instrument(skip(ctx, tether, path, file_name_override))]
    pub async fn create_local(
        ctx: &MailUserContext,
        address_id: AddressId,
        disposition: Disposition,
        path: &Path,
        file_name_override: Option<String>,
        tether: &mut Tether,
    ) -> MailContextResult<Self> {
        debug!("Attachment path: {path:?}");
        let file_metadata = tokio::fs::metadata(path).await?;
        if !file_metadata.is_file() {
            error!("{path:?} is not a file");
            return Err(AppError::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path is not file",
            ))
            .into());
        }
        let Some(file_name) = path.file_name() else {
            error!("{path:?} does not have a file name");
            return Err(AppError::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path does not have a file name",
            ))
            .into());
        };

        // Attachment should be <= 25 MB
        if file_metadata.size() > Attachment::MAX_ATTACHMENT_SIZE {
            return Err(MailContextError::Draft(
                crate::draft::Error::AttachmentUpload(
                    crate::draft::AttachmentUploadError::AttachmentTooLarge,
                ),
            ));
        }
        // File name
        let file_name = file_name_override.unwrap_or(file_name.to_string_lossy().to_string());
        // Determine mime type
        let path_cloned = path.to_owned();
        let mime_type =
            tokio::task::spawn_blocking(move || file_format::FileFormat::from_file(path_cloned))
                .await
                .map_err(|e| MailContextError::Other(anyhow!("Failed to join task: {e:?}")))?
                .map_err(|e| {
                    let e = format!("Failed to determine mime file type: {e:?}");
                    error!("{e}");
                    AppError::InvalidMimeType(e)
                })?;

        let mime_type = MimeType::from_str(mime_type.media_type())?;

        // file size
        let local_address_id = Address::remote_id_counterpart(address_id.clone(), tether).await?;

        let mut attachment = Attachment {
            local_id: None,
            attachment_type: AttachmentType::Remote(None),
            local_address_id,
            remote_address_id: Some(address_id.clone()),
            local_conversation_id: None,
            remote_conversation_id: None,
            local_message_id: None,
            remote_message_id: None,
            disposition,
            content_id: if disposition == Disposition::Inline {
                // Generate a new content id.
                Some(ContentId::new())
            } else {
                None
            },
            enc_signature: None,
            is_auto_forwardee: false,
            key_packets: None,
            mime_type,
            filename: file_name,
            sender: None,
            signature: None,
            size: file_metadata.size(),
            transfer_encoding: None,
            image_width: None,
            image_height: None,
        };

        tether
            .tx(async |tx| {
                trace!("Saving new attachment record");
                attachment.save(tx).await?;

                trace!("Storing attachment in cache");

                let data = tokio::fs::read(path).await?;
                Attachment::store_in_cache(ctx, &attachment.filename, attachment.id(), data, tx)
                    .await
            })
            .await?;

        info!("Attachment created with id {}", attachment.id());
        Ok(attachment)
    }

    pub fn find_by_remote_id_sync(
        attachment_type: &AttachmentType,
        tether: &Connection,
    ) -> Result<Option<Self>, StashError> {
        if let AttachmentType::Remote(Some(_)) = attachment_type {
            let json = attachment_type.to_json()?;
            Attachment::find_first_sync("WHERE attachment_type = ?", (json,), tether)
        } else {
            Ok(None)
        }
    }
    /// Tries to find an attachment by remote id.
    /// This only returns Some if AttachmentType::Remote(Some(_)) and it finds such a record.
    pub async fn find_by_remote_id(
        attachment_type: &AttachmentType,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let attachment_type = attachment_type.clone();
        tether
            .sync_query(move |conn| Self::find_by_remote_id_sync(&attachment_type, conn))
            .await
    }

    /// Return the local id counterpart for a given `remote_id`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    pub async fn remote_id_counterpart(
        remote_id: AttachmentId,
        tether: &Tether,
    ) -> Result<Option<LocalAttachmentId>, StashError> {
        let json = AttachmentType::Remote(Some(remote_id)).to_json()?;

        match tether
            .query_value::<_, LocalAttachmentId>(
                indoc!(
                    "
                    SELECT
                        local_id
                    FROM
                        attachments
                    WHERE
                        attachment_type = ?
                    LIMIT 1
                    ",
                ),
                params![json],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Return the remote id counterpart for a given `local_id`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    pub async fn local_id_counterpart(
        local_attachment_id: LocalAttachmentId,
        tether: &Tether,
    ) -> Result<Option<AttachmentType>, StashError> {
        match tether
            .query_value::<_, AttachmentType>(
                indoc! {"
               SELECT
                    attachment_type
               FROM
                    attachments
               WHERE
                    local_id = ?
               LIMIT 1
               "
                },
                params![local_attachment_id],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Clone an attachment.
    ///
    /// This will create a copy of an existing attachment in the database and the cache.
    ///
    /// # Errors
    ///
    /// Returns error if we can't perform the clone operation or the attachment data is not
    /// in the cache.
    pub async fn clone_attachment_by_id(
        ctx: &MailUserContext,
        address_id: AddressId,
        attachment_id: LocalAttachmentId,
        bond: &Bond<'_>,
    ) -> Result<Attachment, MailContextError> {
        let Some(attachment) = Self::find_by_id(attachment_id, bond).await? else {
            return Err(AppError::AttachmentMissing(attachment_id).into());
        };

        Self::clone_attachment(ctx, address_id, attachment, bond).await
    }

    /// Clone an attachment.
    ///
    /// This will create a copy of an existing attachment in the database and the cache.
    ///
    /// # Errors
    ///
    /// Returns error if we can't perform the clone operation or the attachment data is not
    /// in the cache.
    pub async fn clone_attachment(
        ctx: &MailUserContext,
        address_id: AddressId,
        attachment: Attachment,
        bond: &Bond<'_>,
    ) -> Result<Attachment, MailContextError> {
        let mut new_attachment = attachment;
        let attachment_id = new_attachment.id();

        let Some(current_path) =
            Attachment::path_from_cache_and_update_metadata(attachment_id, bond).await?
        else {
            return Err(AppError::AttachmentIsNotInCache(attachment_id).into());
        };

        new_attachment.local_id = None;
        new_attachment.attachment_type = AttachmentType::Remote(None);
        new_attachment.local_message_id = None;
        new_attachment.remote_message_id = None;
        new_attachment.local_conversation_id = None;
        new_attachment.remote_conversation_id = None;
        new_attachment.local_address_id =
            Address::remote_id_counterpart(address_id.clone(), bond).await?;
        new_attachment.remote_address_id = Some(address_id);
        debug_assert!(new_attachment.local_address_id.is_some());

        new_attachment
            .save(bond)
            .await
            .inspect_err(|e| error!("Failed to stave new attachment: {e:?}"))?;

        Self::copy_attachment_to_cache(
            ctx,
            &new_attachment.filename,
            new_attachment.id(),
            &current_path,
            bond,
        )
        .await
        .inspect_err(|e| error!("Failed to clone pgp attachment in cache: {e:?}"))?;

        Ok(new_attachment)
    }

    pub async fn gen_public_key(
        context: &MailUserContext,
        address: &Address,
        tether: &Tether,
    ) -> Result<PublicKeyAttachment, MailContextError> {
        let pgp = new_pgp_provider();

        let unlocked_address = context
            .unlocked_address_keys(
                &pgp,
                tether,
                address
                    .remote_id
                    .as_ref()
                    .ok_or(AppError::AddressHasNoRemoteId(address.id()))?,
            )
            .await
            .inspect_err(|e| error!("Failed to unlock address: {e:?}"))?;

        let mail_key = unlocked_address
            .primary_for_mail()
            .inspect_err(|e| error!("Failed to get primary mail key: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        let (fingerprint, key) = mail_key
            .export_public_key(&pgp)
            .inspect_err(|e| error!("Failed to export public key: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        let attachment_file_name =
            Self::public_key_attachment_filename(&address.email, fingerprint);

        Ok(PublicKeyAttachment {
            attachment: Attachment {
                local_id: None,
                attachment_type: AttachmentType::Remote(None),
                local_address_id: address.local_id,
                remote_address_id: address.remote_id.clone(),
                local_conversation_id: None,
                remote_conversation_id: None,
                local_message_id: None,
                remote_message_id: None,
                disposition: Disposition::Attachment,
                enc_signature: None,
                is_auto_forwardee: false,
                key_packets: None,
                mime_type: MimeType::application_pgp_keys(),
                filename: attachment_file_name,
                sender: None,
                signature: None,
                size: key.len() as u64,
                content_id: None,
                transfer_encoding: None,
                image_width: None,
                image_height: None,
            },
            key,
        })
    }

    pub async fn create_public_key(
        context: &MailUserContext,
        address: &Address,
        tx: &Bond<'_>,
    ) -> Result<Attachment, MailContextError> {
        let attachment = Self::gen_public_key(context, address, tx).await?;
        attachment.store(context, tx).await
    }

    pub fn public_key_attachment_filename(email: &str, fingerprint: OpenPGPFingerprint) -> String {
        format!(
            "publickey - {} - 0x{}.asc",
            email,
            fingerprint.into_inner().split_at(8).0.to_uppercase()
        )
    }

    pub fn is_public_key_attachment_filename(name: &str) -> bool {
        name.starts_with("publickey - ") && name.ends_with(".asc")
    }

    pub fn is_public_key_attachment(&self) -> bool {
        self.mime_type == MimeType::application_pgp_keys()
            && Self::is_public_key_attachment_filename(&self.filename)
    }

    pub async fn update_after_draft_address_change(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        tx.execute(
            formatdoc!(
                "
            UPDATE {} SET
                key_packets=?,
                local_address_id=?,
                remote_address_id=?,
                signature = NULL,
                enc_signature= NULL
            WHERE local_id =?",
                Self::table_name()
            ),
            params![
                self.key_packets.clone(),
                self.local_address_id,
                self.remote_address_id.clone(),
                self.local_id.unwrap()
            ],
        )
        .await?;
        Ok(())
    }

    pub fn as_inline_img(&self) -> Option<String> {
        self.content_id
            .as_ref()
            .map(|cid| format!(r#"<img src="cid:{cid}" style="max-width: 100%;"><br>"#))
    }
}

impl ModelHooks for Attachment {
    fn before_save(&mut self, tx: &Transaction<'_>) -> stash::stash::StashResult<()> {
        // If we already exist in the db
        if let Some(local_id) = self.local_id {
            // There is currently a race because we try to write too much data at the same time
            // rather than what really changed. It's highly unlikely that we ever want to remove
            // any of these from an attachment that already has one. This happens in the
            // context of drafts, where a local change to the message body metadata can
            // accidentally reset the attachment remote id to nothing, causing the send
            // to fail.
            if let Some(existing) = Attachment::load_by_id_sync(local_id, tx)? {
                if existing.remote_id().is_some() {
                    self.attachment_type = existing.attachment_type;
                }
                if self.key_packets.is_none() {
                    self.key_packets = existing.key_packets;
                }
                if self.enc_signature.is_none() {
                    self.enc_signature = existing.enc_signature;
                }
                if self.signature.is_none() {
                    self.signature = existing.signature;
                }
            } else {
                error!("local_id exists but attachment does not exist in database?!");
            }
        // If another remote attachment exists in the db
        } else if let Some(existing) =
            Attachment::find_by_remote_id_sync(&self.attachment_type, tx)?
        {
            self.local_id = existing.local_id;
        }

        if self.local_address_id.is_none() {
            if let Some(remote_address_id) = &self.remote_address_id {
                self.local_address_id = Address::remote_id_counterpart_sync(remote_address_id, tx)?;
            }
        }

        if self.local_message_id.is_none() {
            if let Some(remote_message_id) = &self.remote_message_id {
                self.local_message_id = Message::remote_id_counterpart_sync(remote_message_id, tx)?;
            }
        }

        if self.local_conversation_id.is_none() {
            if let Some(remote_conversation_id) = &self.remote_conversation_id {
                self.local_conversation_id =
                    Conversation::remote_id_counterpart_sync(remote_conversation_id, tx)?;
            }
        }

        Ok(())
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
            attachment_type: AttachmentType::Remote(Some(value.id)),
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
        }
    }
}

impl From<ApiMessageAttachment> for Attachment {
    fn from(value: ApiMessageAttachment) -> Self {
        Self {
            local_id: None,
            attachment_type: AttachmentType::Remote(Some(value.id)),
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
            content_id: value.headers.content_id.map(ContentId::from),
            transfer_encoding: value.headers.content_transfer_encoding,
            image_width: value.headers.image_width,
            image_height: value.headers.image_height,
        }
    }
}

impl From<AttachmentMetadata> for Attachment {
    fn from(value: AttachmentMetadata) -> Self {
        Self {
            local_id: value.local_id,
            attachment_type: value.attachment_type,
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
        }
    }
}
impl From<Attachment> for AttachmentMetadata {
    fn from(value: Attachment) -> Self {
        Self {
            local_id: value.local_id,
            attachment_type: value.attachment_type,
            disposition: value.disposition,
            mime_type: value.mime_type,
            filename: value.filename,
            size: value.size,
        }
    }
}

pub struct PublicKeyAttachment {
    pub attachment: Attachment,
    pub key: String,
}

impl PublicKeyAttachment {
    pub async fn store(
        mut self,
        context: &MailUserContext,
        tx: &Bond<'_>,
    ) -> Result<Attachment, MailContextError> {
        self.attachment.save(tx).await?;
        Attachment::store_in_cache(
            context,
            &self.attachment.filename,
            self.attachment.id(),
            self.key.into_bytes(),
            tx,
        )
        .await?;
        Ok(self.attachment)
    }
}

#[cfg(test)]
#[path = "../tests/models/attachments.rs"]
mod attachments;
