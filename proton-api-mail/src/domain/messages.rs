use crate::domain::{ApiError, AttachmentId, AttachmentMetadata, ConversationId, Disposition, LabelId};
use crate::exports::serde_json;
use crate::{MailSession, MAX_PAGE_ELEMENT_COUNT};
use proton_api_core::domain::AddressId;
use proton_api_core::exports::serde::{self, Deserialize, Serialize, Serializer};
use proton_api_core::utils::{bool_from_integer, bool_to_integer, opt_bool_to_integer};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};
use stash::exports::{FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, ValueRef};
use stash::macros::Model;
use stash::orm::Model;
use stash::{params, sql_using_serde};
use stash::stash::{Stash, StashError};
use tracing::{debug, error};
use crate::requests::{GetMessageMetadataRequest, GetMessageRequest};
use serde_json::Value as JsonValue;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::UnlockedAddressKeys;
use std::convert::AsRef;
use indoc::formatdoc;

proton_api_core::utils::string_id!(MessageId);
proton_api_core::utils::string_id!(ExternalId);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Default)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MessageAddress {
    //TODO: Proper email parsing
    pub address: String,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_proton: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub display_sender_image: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_simple_login: bool,
    pub bimi_selector: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
#[serde(crate = "self::serde", transparent)]
#[repr(transparent)]
pub struct MessageFlags(u64);

#[cfg(feature = "uniffi")]
uniffi::custom_newtype!(MessageFlags, u64);

bitflags::bitflags! {
    impl MessageFlags:u64 {
        /// Whether a message is received.
        const RECEIVED = 1;
        /// Whether a message is sent.
        const SENT = 1 << 1;
        /// Whether the message is between Proton Mail Recipients.
        const INTERNAL = 1<< 2;
        /// Whether the message is end-to-end encrypted.
        const E2E = 1 << 3;
        /// Whether the message is an auto response.
        const AUTO = 1 << 4;
        /// Whether the message is replied to.
        const REPLIED = 1 << 5;
        /// Whether the message is replied to all.
        const REPLIED_ALL = 1 << 6;
        /// Whether the message is forwarded.
        const FORWARDED = 1 << 7;
        /// Whether the message has been responded with an auto response.
        const AUTO_REPLIED = 1 << 8;
        /// Whether the message is an import.
        const IMPORTED = 1 << 9;
        /// Whether the message has ever been opened by the user.
        const OPENED = 1 << 10;
        /// Whether a read receipt has been sent in response to the message.
        const RECEIPT_SENT = 1 << 11;
        /// No longer used.
        const UNUSED_1 = 1 << 12;
        /// No longer used.
        const UNUSED_2 = 1 << 13;
        /// Whether the message is a receipt.
        const RECEIPT = 1 <<14;
        /// Whether the message is from proton.
        const PROTON = 1 << 15;
        /// Whether to request a read receipt for the message.
        const RECEIPT_REQUEST = 1 << 16;
        /// Whether to attach public key.
        const PUBLIC_KEY = 1 << 17;
        /// Whether to sing the message.
        const SIGN = 1 << 18;
        /// Unsubscribed from newsletter.
        const UNSUBSCRIBED = 1 << 19;
        /// Messages that been scheduled to send at a later time.
        const SCHEDULED_SEND = 1 << 20;
        /// No longer used.
        const UNUSED_3 = 1 << 21;
        /// Whether the message was synced from gmail.
        const SYNCED_FROM_GMAIL = 1 << 22;
        /// Whether DMARC authentication passed.
        const DMARC_PASS = 1 << 23;
        /// Whether message failed SPF check.
        const SPF_FAIL = 1 << 24;
        /// Whether message failed DKIM check.
        const DKIM_FAIL = 1 << 25;
        /// Whether incoming message failed DMARC authentication.
        const DMARC_FAIL = 1  << 26;
        /// Whether the message is in spam and the user moves it to a new location that is not
        /// spam or trash (e.g. inbox or archive).
        const HAM_MANUAL = 1 << 27;
        /// Whether the message is marked as spam by anti-spam filters.
        const SPAM_AUTO = 1 << 28;
        /// Whether the message has been manually marked as spam.
        const SPAM_MANUAL = 1 <<29;
        /// Whether the message is marked as phishing by anti-spam filters.
        const PHISHING_AUTO = 1 << 30;
        /// Whether the message has been manually marked as phishing.
        const PHISHING_MANUAL = 1 << 31;
        /// Messages where the expiration time cannot be changed.
        const FROZEN_EXPIRATION= 1 << 32;
        /// Whether the message has been flagged as suspicious by the system.
        const FLAG_SUSPICIOUS = 1 << 33;
        /// Whether message is auto-forwarded.
        const FLAG_AUTO_FORWARDER = 1 << 34;
        /// Whether message is auto-forwarded.
        const FLAG_AUTO_FORWARDEE = 1 << 35;
    }
}

impl MessageFlags {
    /// Check whether this message is an auto-sent reply.
    #[must_use]
    pub fn is_sent_auto(&self) -> bool {
        if !self.intersects(MessageFlags::SENT) {
            return false;
        }

        self.intersects(MessageFlags::AUTO)
    }

    /// Check whether this message is a draft.
    #[must_use]
    pub fn is_draft(&self) -> bool {
        !self.intersects(MessageFlags::SENT | MessageFlags::RECEIVED)
    }
}

#[cfg(feature = "sql")]
impl ToSql for MessageFlags {
    fn to_sql(
        &self,
    ) -> Result<
        ToSqlOutput<'_>, SqliteError,
    > {
        self.0.to_sql()
    }
}

#[cfg(feature = "sql")]
impl FromSql for MessageFlags {
    fn column_result(
        value: ValueRef<'_>,
    ) -> FromSqlResult<Self> {
        let value = u64::column_result(value)?;
        MessageFlags::from_bits(value)
            .ok_or(FromSqlError::InvalidType)
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct MessageMetadata {
    #[serde(rename = "ID")]
    pub remote_id: MessageId,
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,
    pub order: u64,
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    #[serde(rename = "LabelIDs")]
    pub label_ids: LabelIds,
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,

    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub sender: MessageAddress,
    #[serde(default)]
    pub to_list: MessageAddresses,
    #[serde(rename = "CCList", default)]
    pub cc_list: MessageAddresses,
    #[serde(rename = "BCCList", default)]
    pub bcc_list: MessageAddresses,
    #[serde(default)]
    pub reply_tos: MessageAddresses,
    pub flags: MessageFlags,
    pub time: u64,
    pub size: u64,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub unread: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_replied: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_replied_all: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_forwarded: bool,
    pub expiration_time: u64,
    pub snooze_time: u64,
    pub num_attachments: u32,
    #[serde(default)]
    pub attachments_metadata: AttachmentMetadatas,
}

sql_using_serde!(MessageMetadata);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[serde(crate = "self::serde")]
pub enum MimeType {
    #[serde(rename = "text/plain")]
    TextPlain,
    #[serde(rename = "text/html")]
    TextHTML,
    #[serde(rename = "multipart/mixed")]
    MultipartMixed,
    #[serde(rename = "multipart/related")]
    MultipartRelated,
    #[serde(rename = "message/rfc822")]
    MessageRFC822,
}

#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
#[TableName("messages")]
pub struct Message {
	#[IdField(autoincrement)]
	#[serde(skip)]
	pub local_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: Option<MessageId>,
    #[DbField]
    pub local_conversation_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ConversationID")]
    pub remote_conversation_id: ConversationId,
    #[DbField]
    pub order: u64,
    #[DbField]
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    #[DbField]
    #[serde(rename = "LabelIDs")]
    pub label_ids: LabelIds,
    #[DbField]
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,
    #[DbField]
    #[serde(default)]
    pub subject: String,
    #[DbField]
    #[serde(default)]
    pub sender: MessageAddress,
    #[DbField]
    #[serde(default)]
    pub to_list: MessageAddresses,
    #[DbField]
    #[serde(rename = "CCList", default)]
    pub cc_list: MessageAddresses,
    #[DbField]
    #[serde(rename = "BCCList", default)]
    pub bcc_list: MessageAddresses,
    #[DbField]
    #[serde(default)]
    pub reply_tos: MessageAddresses,
    #[DbField]
    pub flags: MessageFlags,
    #[DbField]
    pub time: u64,
    #[DbField]
    pub size: u64,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub unread: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_replied: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_replied_all: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub is_forwarded: bool,
    #[DbField]
    pub expiration_time: u64,
    #[DbField]
    pub snooze_time: u64,
    #[DbField]
    pub num_attachments: u32,
    #[DbField]
    #[serde(default)]
    pub attachments_metadata: AttachmentMetadatas,
    #[DbField]
    pub header: String,
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    #[DbField]
    pub parsed_headers: ParsedHeaders,
    pub body: String,
    #[DbField]
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,
    #[DbField]
    #[serde(default)]
    pub attachments: MessageAttachments,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

impl Message {
    async fn create_or_update_messages_from_metadata(
        metadata: Vec<MessageMetadata>,
        stash: &Stash,
    ) -> Result<Vec<u64>, ApiError> {
        let tx = stash.transaction().await?;
        let mut ids = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            let mut message = Self {
                local_id: None,
                remote_id: Some(metadata.remote_id),
                local_conversation_id: None,
                remote_conversation_id: metadata.conversation_id,
                order: metadata.order,
                address_id: metadata.address_id,
                label_ids: metadata.label_ids,
                external_id: metadata.external_id,
                subject: metadata.subject,
                sender: metadata.sender,
                to_list: metadata.to_list,
                cc_list: metadata.cc_list,
                bcc_list: metadata.bcc_list,
                reply_tos: metadata.reply_tos,
                flags: metadata.flags,
                time: metadata.time,
                size: metadata.size,
                unread: metadata.unread,
                is_replied: metadata.is_replied,
                is_replied_all: metadata.is_replied_all,
                is_forwarded: metadata.is_forwarded,
                expiration_time: metadata.expiration_time,
                snooze_time: metadata.snooze_time,
                num_attachments: metadata.num_attachments,
                attachments_metadata: metadata.attachments_metadata,
                header: "".to_owned(),
                parsed_headers: ParsedHeaders {
                    headers: HashMap::new(),
                },
                body: "".to_owned(),
                mime_type: MimeType::TextPlain,
                attachments: MessageAttachments(Vec::new()),
                row_id: None,
                stash: Some(stash.clone()),
            };
            if let Some(existing) = Self::find("WHERE remote_id = ?", params![message.remote_id.clone()], stash, None).await?.into_iter().next() {
                message.local_id = existing.local_id;
                message.row_id = existing.row_id;
                message.stash = existing.stash;
                
                // Remove any labels that are no longer associated with this conversation.
                if !message.label_ids.0.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(formatdoc!("
                        DELETE FROM
                            message_labels
                        WHERE
                            local_message_id = ?
                            AND local_label_id NOT IN (
                                SELECT local_id FROM labels WHERE remote_id IN ({})
                            )
                        ",
                        vec!["?"; message.label_ids.0.len()].join(",")
                    ), vec![Box::new(message.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>].into_iter().chain(message.label_ids.0.iter().map(|label| Box::new(label.clone()) as Box<dyn ToSql + Send>)).collect()).await?;
                } else {
                    tx.execute(formatdoc!("
                        DELETE FROM
                            message_labels
                        WHERE
                            local_message_id = ?
                        ",
                    ), params![message.local_id]).await?;
                }

                // Remove any attachments that are no longer associated with this conversation.
                if !message.attachments_metadata.0.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(formatdoc!("
                        DELETE FROM
                            message_attachments
                        WHERE
                            local_message_id = ?
                            AND local_attachment_id NOT IN ({})
                        ",
                        vec!["?"; message.attachments_metadata.0.len()].join(",")
                    ), vec![Box::new(message.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>].into_iter().chain(message.attachments_metadata.0.iter().map(|attachment| Box::new(attachment.remote_id.clone()) as Box<dyn ToSql + Send>)).collect()).await?;
                } else {
                    tx.execute(formatdoc!("
                        DELETE FROM
                            message_attachments
                        WHERE
                            local_message_id = ?
                        ",
                    ), params![message.local_id]).await?;
                }
            }
            message.save_using(&tx).await?;
            
            for mut _label in message.label_ids.0 {
                // TODO
                // label.save_using(&tx).await?;
                continue;
            }
            for mut _attachment in message.attachments_metadata.0 {
                // TODO
                // attachment.save_using(&tx).await?;
                continue;
            }

            ids.push(message.local_id.unwrap());
        }
        tx.commit().await?;
        Ok(ids)
    }
    
    /// Get the cache path for a message body with `id`.
    pub fn message_cache_path(&self, cache_path: &Path) -> PathBuf {
        cache_path.join(format!("message_body_{}", self.local_id.expect("Message does not have a local id")))
    }
    
    /// Get the message's body.
    ///
    /// This will attempt to fetch the message data from the servers if it has
    /// not yet been downloaded before.
    ///
    /// # Errors
    /// 
    /// Returns error if the message failed to download, the db query failed or
    /// the message body could not be written to the cache.
    /// 
    pub async fn message_body<P: PGPProviderSync>(
        &self,
        cache_path: &Path,
        mail_session: &MailSession,
        pgp_provider: P,
        address_keys: UnlockedAddressKeys<P>,
    ) -> Result<DecryptedMessageBody, ApiError>
    where
        UnlockedAddressKeys<P>: AsRef<P::PrivateKey>
    {
        // Fetch metadata first to sync contents and cache.
        let metadata = self.sync_message_body(cache_path, mail_session).await?;

        // TODO(ET-231): Read body from cache.
        let encrypted_body = std::fs::read_to_string(self.message_cache_path(cache_path)).map_err(|e| {
            error!("Failed to read encrypted message body from cache: {e}");
            ApiError::Other(r#"MailboxError::Context(MailContextError::Other(anyhow!("{e}")))"#.to_owned())
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
                ApiError::Other("e".to_owned())
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

    /// Synchronize the message body.
    ///
    /// # Errors
    /// 
    /// Returns error if the API request failed or the data could not be written
    /// to the database.
    /// 
    pub async fn sync_message_body(
        &self,
        cache_path: &Path,
        mail_session: &MailSession,
    ) -> Result<MessageBodyMetadata, ApiError> {
        let Some(conn) = self.stash() else {
            return Err(ApiError::Stash(StashError::NoStashAvailable));
        };
        // TODO(ET-231): Use caching solution.
        let metadata = if let Some(metadata) = MessageBodyMetadata::find("WHERE id = ?", params![self.local_id], &conn, None)
            .await
            .map_err(|e| {
                error!("Failed to retrieve message body metadata from db: {e}");
                e
            })?.into_iter().next() {
            metadata
        } else {
            // metadata is not there it is either missing or the message does not exist.
            let remote_id =
                self.remote_id.clone().ok_or(ApiError::Other("MailboxError::MessageDoesNotHaveRemoteId(self.local_id)".to_owned()))?;
            // sync the message body
            let message = mail_session.session()
                .execute_request(GetMessageRequest::new(&remote_id))
                .await
                .map(|v| v.message)
                .map_err(|e| {
                    error!("Failed to retrieve message: {e}");
                    ApiError::Other("MailContextError::from(e)".to_owned())
                })?;

            // create message in the database and store body in the cache.
                let mut metadata = MessageBodyMetadata {
                    local_message_id: message.local_id,
                    remote_id: message.remote_id.clone(),
                    header: message.header.clone(),
                    parsed_headers: message.parsed_headers.clone(),
                    mime_type: message.mime_type.clone(),
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
                    ApiError::Other(r#"MailboxError::Context(MailContextError::Other(anyhow!("{e}")))"#.to_owned())
                })?;

                metadata
        };

        Ok(metadata)
    }
    
    /// Synchronize the first `count` messages of the label with `label_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_first_message_page(
        label_id: LabelId,
        count: usize,
        stash: &Stash,
        session: &MailSession,
    ) -> Result<(), ApiError> {
        let response = session.session()
            .execute_request(GetMessageMetadataRequest::new(MessageMetadataFilter {
                page: 0,
                page_size: count.max(MAX_PAGE_ELEMENT_COUNT) as u64,
                label_id: Some(vec![label_id]),
                desc: Some(true),
                ..Default::default()
			}))
            .await?;

        debug!(
            "Fetched {} messages TOTAL={}",
            response.messages.len(),
            response.total
        );

        Self::create_or_update_messages_from_metadata(response.messages, stash).await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageAttachments(pub Vec<MessageAttachment>);

sql_using_serde!(MessageAttachments);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageAddresses(pub Vec<MessageAddress>);

sql_using_serde!(MessageAddresses);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct AttachmentMetadatas(pub Vec<AttachmentMetadata>);

sql_using_serde!(AttachmentMetadatas);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct LabelIds(pub Vec<LabelId>);

sql_using_serde!(LabelIds);

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageAttachment {
    #[serde(rename = "ID")]
    pub id: AttachmentId,
    pub name: String,
    pub size: u64,
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    pub disposition: Disposition,
    pub key_packets: KeyPackets,
    pub signature: Option<AttachmentSignature>,
    pub enc_signature: Option<AttachmentEncryptedSignature>,
    pub headers: MessageAttachmentHeaders,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde")]
pub struct MessageAttachmentHeaders {
    #[serde(rename = "content-disposition")]
    pub content_disposition: String,
    #[serde(rename = "content-id")]
    pub content_id: Option<String>,
    #[serde(rename = "content-transfer-encoding")]
    pub content_transfer_encoding: Option<String>,
    #[serde(rename = "x-pm-image-width")]
    pub image_width: Option<String>,
    #[serde(rename = "x-pm-image-height")]
    pub image_height: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Copy)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageAttachmentInfo {
    #[serde(default)]
    pub attachment: u32,
    #[serde(default)]
    pub inline: u32,
}

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum MessageMetadataSortMode {
    Time,
    Size,
    ID,
}

impl std::fmt::Display for MessageMetadataSortMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageMetadataSortMode::Time => {
                write!(f, "Time")
            }
            MessageMetadataSortMode::Size => {
                write!(f, "Size")
            }
            MessageMetadataSortMode::ID => {
                write!(f, "ID")
            }
        }
    }
}

impl Serialize for MessageMetadataSortMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MessageMetadataSortMode::ID => serializer.serialize_str("ID"),
            MessageMetadataSortMode::Time => serializer.serialize_str("Time"),
            MessageMetadataSortMode::Size => serializer.serialize_str("Size"),
        }
    }
}

/// Metadata associated with the Body of a message.
///
/// Message bodies are not stored in the database.
///
/// For metadata associated with a message see [`MessageMetadata`].
/// 
#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("message_bodies")]
pub struct MessageBodyMetadata {
    #[IdField(optional)]
    #[serde(skip)]
    pub local_message_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: Option<MessageId>,
    #[DbField]
    pub header: String,
    #[DbField]
    pub parsed_headers: ParsedHeaders,
    #[DbField]
    pub mime_type: MimeType,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

#[derive(Clone, Debug, Eq, Deserialize, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ParsedHeaders {
    pub headers: HashMap<String, serde_json::Value>,
}

sql_using_serde!(ParsedHeaders);

/// Parameters to filter/search messages with a given criteria.
#[derive(Debug, Default, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MessageMetadataFilter {
    /// Page index.
    pub page: u64,
    /// Number of elements per page.
    pub page_size: u64,
    /// The number of messages to return.
    pub limit: Option<u64>,
    /// Label ids to filter on.
    #[serde(rename = "LabelID")]
    pub label_id: Option<Vec<LabelId>>,
    /// Result sort mode.
    pub sort: Option<MessageMetadataSortMode>,
    /// If true sort results descending. If false, sort ascending.
    #[serde(
        deserialize_with = "opt_bool_from_integer",
        serialize_with = "opt_bool_to_integer"
    )]
    pub desc: Option<bool>,
    /// UNIX timestamp to filter messages at or later than timestamp.
    pub begin: Option<u64>,
    /// UNIX timestamp to filter messages at or earlier than timestamp.
    pub end: Option<u64>,
    /// Return only messages newer, in creation time (NOT timestamp), than `begin_id`.
    #[serde(rename = "BeginID")]
    pub begin_id: Option<MessageId>,
    /// Return only messages older, in creation time (NOT timestamp), than `end_id`.
    #[serde(rename = "EndID")]
    pub end_id: Option<MessageId>,
    /// Keyword search of To, CC, BCC, From and Subject fields.
    pub keyword: Option<String>,
    /// Keyword search of To, CC and BCC fields.
    pub recipients: Option<Vec<String>>,
    /// Keyword search of To field.
    pub to: Option<String>,
    /// Keyword search of CC field.
    #[serde(rename = "CC")]
    pub cc: Option<String>,
    /// Keyword search of BCC field.
    #[serde(rename = "BCC")]
    pub bcc: Option<String>,
    /// Keyword search From field.
    pub from: Option<String>,
    /// Keyword search Subject field.
    pub subject: Option<String>,
    /// If true return only messages which have attachments. If false return only messages which
    /// have no attachments.
    #[serde(
        deserialize_with = "opt_bool_from_integer",
        serialize_with = "opt_bool_to_integer"
    )]
    pub attachments: Option<bool>,
    /// If true return only messages which are unread. If false return only messages which are read.
    #[serde(
        deserialize_with = "opt_bool_from_integer",
        serialize_with = "opt_bool_to_integer"
    )]
    pub unread: Option<bool>,
    /// Filter messages by `conversation_id`.
    #[serde(rename = "ConversationID")]
    pub conversation_id: Option<ConversationId>,
    /// Filter on address id.
    #[serde(rename = "AddressID")]
    pub address_id: Option<AddressId>,
    /// Filter on external id.
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,
    #[serde(rename = "ID")]
    /// Filter on the given message ids.
    pub ids: Option<Vec<MessageId>>,
    /// If true automatically convert simple queries to wildcarded versions, such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageCount {
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
    pub total: u64,
    pub unread: u64,
}

#[cfg(feature = "sql")]
impl ToSql for MimeType {
    fn to_sql(
        &self,
    ) -> Result<
        ToSqlOutput<'_>, SqliteError,
    > {
        match self {
            MimeType::TextPlain => "text/plain",
            MimeType::TextHTML => "text/html",
            MimeType::MultipartMixed => "multipart/mixed",
            MimeType::MultipartRelated => "multipart/related",
            MimeType::MessageRFC822 => "message/rfc822",
        }
        .to_sql()
    }
}

#[cfg(feature = "sql")]
impl FromSql for MimeType {
    fn column_result(
        value: ValueRef<'_>,
    ) -> FromSqlResult<Self> {
        let value = value.as_str()?;
        Ok(match value {
            "text/plain" => MimeType::TextPlain,
            "text/html" => MimeType::TextHTML,
            "multipart/mixed" => MimeType::MultipartMixed,
            "multipart/related" => MimeType::MultipartRelated,
            "message/rfc822" => MimeType::MessageRFC822,
            _ => {
                return Err(
                    FromSqlError::Other(
                        format!("invalid mime type value:{value}").into(),
                    ),
                )
            }
        })
    }
}

/// Consists of the message's body metadata and decrypted content.
pub struct DecryptedMessageBody {
    /// Metadata associated with the message body
    pub metadata: MessageBodyMetadata,
    /// The decrypted message contents.
    pub body: String,
}

/// A message parsed header value can either be a string or an array of strings.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ParsedHeaderValue {
    String(String),
    Array(Vec<String>),
}

impl DecryptedMessageBody {
    /// Retrieve a parsed header value for a given `key`.
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        let value = self.metadata.parsed_headers.headers.get(key)?;
        match value {
            JsonValue::String(s) => Some(ParsedHeaderValue::String(s.clone())),
            JsonValue::Array(array) => {
                let mut result = Vec::with_capacity(array.len());
                for (idx, item) in array.iter().enumerate() {
                    if let JsonValue::String(str) = item {
                        result.push(str.clone());
                    } else {
                        tracing::warn!(
                            "Header array value {key}[{idx}] of message {:?} has invalid value type",
                            self.metadata.local_message_id
                        );
                    }
                }
                Some(ParsedHeaderValue::Array(result))
            }
            _ => {
                tracing::warn!(
                    "Header value {key} of message {:?} has invalid value type",
                    self.metadata.local_message_id
                );
                None
            }
        }
    }
}

struct EncryptedMessageBody {
    pub metadata: MessageBodyMetadata,
    pub encrypted_body: String,
}

impl DecryptableMessage for EncryptedMessageBody {
    fn message_id(&self) -> Option<&str> {
        self.metadata.remote_id.as_ref().map(|v| v.as_ref())
    }

    fn message_is_mime(&self) -> bool {
        self.metadata.mime_type == MimeType::MultipartMixed
    }

    fn message_encrypted_body(&self) -> &[u8] {
        self.encrypted_body.as_bytes()
    }
}
