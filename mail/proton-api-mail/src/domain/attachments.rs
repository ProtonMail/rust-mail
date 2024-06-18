use crate::domain::{ApiError, ConversationId, MessageAddress, MessageId};
use crate::exports::serde::{Deserializer, Serializer};
use crate::MailSession;
use proton_api_core::{
    domain::AddressId,
    exports::serde::{self, Deserialize, Serialize},
};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, ValueRef,
};
use stash::macros::Model;
use stash::orm::Model;
use stash::sql_using_serde;
use stash::stash::Stash;
use std::ops::Deref;

proton_api_core::utils::string_id!(AttachmentId);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(crate = "self::serde", rename_all = "snake_case")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Disposition {
    Inline,
    Attachment,
}

#[cfg(feature = "sql")]
impl FromSql for Disposition {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "inline" => Ok(Disposition::Inline),
            "attachment" => Ok(Disposition::Attachment),
            _ => Err(FromSqlError::Other("Invalid enum value".into())),
        }
    }
}

#[cfg(feature = "sql")]
impl ToSql for Disposition {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(match self {
            Disposition::Inline => "inline".as_bytes(),
            Disposition::Attachment => "attachment".as_bytes(),
        })))
    }
}

/// Wrapper type around `RealAttachmentEncryptedSignature`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentEncryptedSignature(pub RealAttachmentEncryptedSignature);

impl Deref for AttachmentEncryptedSignature {
    type Target = RealAttachmentEncryptedSignature;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AttachmentEncryptedSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let real_signature = RealAttachmentEncryptedSignature::deserialize(deserializer)?;
        Ok(AttachmentEncryptedSignature(real_signature))
    }
}

impl Serialize for AttachmentEncryptedSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Wrapper type around `RealAttachmentSignature`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentSignature(pub RealAttachmentSignature);

impl Deref for AttachmentSignature {
    type Target = RealAttachmentSignature;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AttachmentSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let real_signature = RealAttachmentSignature::deserialize(deserializer)?;
        Ok(AttachmentSignature(real_signature))
    }
}

impl Serialize for AttachmentSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Wrapper type around `RealAttachmentEncryptedSignature`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyPackets(pub RealKeyPackets);

impl Deref for KeyPackets {
    type Target = RealKeyPackets;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for KeyPackets {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let real_key_packets = RealKeyPackets::deserialize(deserializer)?;
        Ok(KeyPackets(real_key_packets))
    }
}

impl Serialize for KeyPackets {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("attachments")]
pub struct Attachment {
    #[IdField(autoincrement)]
    #[serde(skip)]
    pub local_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: Option<AttachmentId>,
    #[DbField]
    pub name: String,
    #[DbField]
    pub size: u64,
    #[DbField]
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    #[DbField]
    pub disposition: Disposition,
    #[DbField]
    pub key_packets: KeyPackets,
    #[DbField]
    pub signature: Option<AttachmentSignature>,
    #[DbField]
    pub enc_signature: Option<AttachmentEncryptedSignature>,
    #[DbField]
    pub sender: Option<MessageAddress>,
    #[DbField]
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    #[DbField]
    #[serde(rename = "MessageID")]
    pub message_id: MessageId,
    #[DbField]
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,
    #[DbField]
    pub is_auto_forwardee: bool,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

sql_using_serde!(AttachmentEncryptedSignature);
sql_using_serde!(AttachmentSignature);
sql_using_serde!(KeyPackets);
sql_using_serde!(MessageAddress);

impl Attachment {
    /// Check whether attachment is complete.
    ///
    /// Attachment metadata is considered complete when all the information
    /// required to decrypt the attachment is in the database. When storing
    /// conversation/messages into the database we only get partial data for the
    /// attachment.
    ///
    /// To complete the data, one needs to provide the full metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    ///
    pub fn has_complete_metadata(&self) -> bool {
        self.key_packets.to_string().len() > 0
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
    pub async fn sync_complete_metadata(
        &self,
        session: &MailSession,
    ) -> Result<Option<()>, ApiError> {
        let remote_attachment_id = if let Some(remote_id) = self.remote_id.clone() {
            remote_id
        } else {
            return Ok(None);
        };
        let mut attachment = session
            .attachment_metadata_complete(remote_attachment_id)
            .await?
            .attachment;
        attachment.local_id = self.local_id;
        attachment.row_id = self.row_id;
        attachment.stash = self.stash.clone();
        attachment.save().await?;
        Ok(Some(()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AttachmentMetadata {
    #[serde(rename = "ID")]
    pub remote_id: AttachmentId,
    pub size: u64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    pub disposition: Disposition,
}
