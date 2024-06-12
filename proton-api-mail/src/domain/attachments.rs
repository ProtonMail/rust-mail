use crate::domain::{ConversationId, MessageAddress, MessageId};
use proton_api_core::{
    domain::AddressId,
    exports::serde::{self, Deserialize, Serialize},
};

proton_api_core::utils::string_id!(AttachmentId);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(crate = "self::serde", rename_all = "snake_case")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Disposition {
    Inline,
    Attachment,
}

#[cfg(feature = "sql")]
use proton_api_core::exports::proton_sqlite3::rusqlite;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};

#[cfg(feature = "sql")]
impl rusqlite::types::FromSql for Disposition {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "inline" => Ok(Disposition::Inline),
            "attachment" => Ok(Disposition::Attachment),
            _ => Err(rusqlite::types::FromSqlError::Other(
                "Invalid enum value".into(),
            )),
        }
    }
}

#[cfg(feature = "sql")]
impl rusqlite::types::ToSql for Disposition {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(
            rusqlite::types::ValueRef::Text(match self {
                Disposition::Inline => "inline".as_bytes(),
                Disposition::Attachment => "attachment".as_bytes(),
            }),
        ))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct Attachment {
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
    pub sender: Option<MessageAddress>,
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    #[serde(rename = "MessageID")]
    pub message_id: MessageId,
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,
    pub is_auto_forwardee: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AttachmentMetadata {
    #[serde(rename = "ID")]
    pub id: AttachmentId,
    pub size: u64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    pub disposition: Disposition,
}
