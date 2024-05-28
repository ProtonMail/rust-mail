use crate::domain::{AttachmentId, AttachmentMetadata, ConversationId, Disposition, LabelId};
use crate::exports::serde_json;
use crate::MAX_PAGE_ELEMENT_COUNT;
use proton_api_core::domain::AddressId;
use proton_api_core::exports::serde::{self, Deserialize, Serialize, Serializer};
use proton_api_core::utils::{bool_from_integer, bool_to_integer, opt_bool_to_integer};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use std::collections::HashMap;

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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct MessageMetadata {
    #[serde(rename = "ID")]
    pub id: MessageId,
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,
    pub order: u64,
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,

    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub sender: MessageAddress,
    #[serde(default)]
    pub to_list: Vec<MessageAddress>,
    #[serde(rename = "CCList", default)]
    pub cc_list: Vec<MessageAddress>,
    #[serde(rename = "BCCList", default)]
    pub bcc_list: Vec<MessageAddress>,
    #[serde(default)]
    pub reply_tos: Vec<MessageAddress>,
    pub flags: u64,
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
    pub attachments_metadata: Vec<AttachmentMetadata>,
}

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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Message {
    #[serde(flatten)]
    pub metadata: MessageMetadata,
    pub header: String,
    // Unfortunately, some values returned in this struct are either
    // arrays or strings.
    pub parsed_headers: HashMap<String, serde_json::Value>,
    pub body: String,
    #[serde(rename = "MIMEType")]
    pub mime_type: MimeType,
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,
}

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

/// Parameters to filter/search messages with a given criteria.
#[derive(Debug, Serialize)]
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
    ids: Option<Vec<MessageId>>,
    /// If true automatically convert simple queries to wildcarded versions, such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,
}

impl MessageMetadataFilter {
    fn new(page_number: usize, page_size: usize) -> Self {
        Self {
            ids: None,
            subject: None,
            attachments: None,
            address_id: None,
            external_id: None,
            end_id: None,
            keyword: None,
            recipients: None,
            to: None,
            cc: None,
            bcc: None,
            label_id: None,
            sort: None,
            desc: None,
            begin: None,
            end: None,
            conversation_id: None,
            page_size: page_size.max(MAX_PAGE_ELEMENT_COUNT) as u64,
            page: page_number as u64,
            limit: None,
            begin_id: None,
            from: None,
            unread: None,
            auto_wildcard: None,
        }
    }
}

/// Builder for [`MessageMetadataFilter`].
#[derive(Debug)]
pub struct MessageMetadataFilterBuilder(MessageMetadataFilter);

impl MessageMetadataFilterBuilder {
    /// Create a new builder for `page_index` and with a `page_size` number of elements.
    #[must_use]
    pub fn new(page_index: usize, page_size: usize) -> Self {
        Self(MessageMetadataFilter::new(page_index, page_size))
    }

    /// The number of messages to return.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.0.limit = Some(limit as u64);
        self
    }

    /// The `label_ids` to filter on.
    ///
    /// This function is cumulative and does not reset previous values.
    #[must_use]
    pub fn with_label_id(mut self, label_ids: impl Into<LabelId>) -> Self {
        match &mut self.0.label_id {
            None => {
                self.0.label_id = Some(vec![label_ids.into()]);
            }
            Some(v) => {
                v.push(label_ids.into());
            }
        };
        self
    }

    /// Result sort `mode`.
    #[must_use]
    pub fn with_sort_mode(mut self, mode: MessageMetadataSortMode) -> Self {
        self.0.sort = Some(mode);
        self
    }

    /// Sort the results descending.
    #[must_use]
    pub fn descending(mut self) -> Self {
        self.0.desc = Some(true);
        self
    }

    /// Sort the results ascending.
    #[must_use]
    pub fn ascending(mut self) -> Self {
        self.0.desc = Some(false);
        self
    }

    /// UNIX timestamp to filter messages at or later than `timestamp`.
    #[must_use]
    pub fn with_begin(mut self, timestamp: u64) -> Self {
        self.0.begin = Some(timestamp);
        self
    }

    /// UNIX timestamp to filter messages at or earlier than `timestamp`.
    #[must_use]
    pub fn with_end(mut self, timestamp: u64) -> Self {
        self.0.end = Some(timestamp);
        self
    }

    /// Return only messages newer, in creation time (NOT timestamp), than `begin_id`.
    #[must_use]
    pub fn with_begin_id(mut self, begin_id: impl Into<MessageId>) -> Self {
        self.0.begin_id = Some(begin_id.into());
        self
    }

    /// Return only messages older, in creation time (NOT timestamp), than `end_id`.
    #[must_use]
    pub fn with_end_id(mut self, end_id: impl Into<MessageId>) -> Self {
        self.0.end_id = Some(end_id.into());
        self
    }

    /// Keyword search of To, CC, BCC, From and Subject fields.
    #[must_use]
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.keyword = Some(keyword.into());
        self
    }

    /// Keyword search of To, CC and BCC fields.
    #[must_use]
    pub fn with_recipients(mut self, recipients: impl IntoIterator<Item = String>) -> Self {
        self.0.recipients = Some(recipients.into_iter().collect());
        self
    }

    /// Keyword search of To field.
    #[must_use]
    pub fn with_to(mut self, keyword: impl Into<String>) -> Self {
        self.0.to = Some(keyword.into());
        self
    }

    /// Keyword search of CC field.
    #[must_use]
    pub fn with_cc(mut self, keyword: impl Into<String>) -> Self {
        self.0.cc = Some(keyword.into());
        self
    }

    /// Keyword search of CC field.
    #[must_use]
    pub fn with_bcc(mut self, keyword: impl Into<String>) -> Self {
        self.0.bcc = Some(keyword.into());
        self
    }

    /// Keyword search of From field.
    #[must_use]
    pub fn with_from(mut self, keyword: impl Into<String>) -> Self {
        self.0.from = Some(keyword.into());
        self
    }

    /// Keyword search of Subject field.
    #[must_use]
    pub fn with_subject(mut self, keyword: impl Into<String>) -> Self {
        self.0.subject = Some(keyword.into());
        self
    }

    /// Return only message which have attachments.
    #[must_use]
    pub fn with_attachments(mut self) -> Self {
        self.0.attachments = Some(true);
        self
    }

    /// Return only message which have no attachments.
    #[must_use]
    pub fn without_attachments(mut self) -> Self {
        self.0.attachments = Some(false);
        self
    }

    /// Return only messages that are unread.
    #[must_use]
    pub fn with_unread(mut self) -> Self {
        self.0.unread = Some(true);
        self
    }

    /// Return only messages that are read.
    #[must_use]
    pub fn with_read(mut self) -> Self {
        self.0.unread = Some(false);
        self
    }

    /// Filter message by `conversation_id`.
    #[must_use]
    pub fn with_conversation_id(mut self, conversation_id: impl Into<ConversationId>) -> Self {
        self.0.conversation_id = Some(conversation_id.into());
        self
    }

    /// Filter on `address_id`.
    #[must_use]
    pub fn with_address_id(mut self, address_id: impl Into<AddressId>) -> Self {
        self.0.address_id = Some(address_id.into());
        self
    }

    /// Filter on `external_id`.
    #[must_use]
    pub fn with_external_id(mut self, external_id: impl Into<ExternalId>) -> Self {
        self.0.external_id = Some(external_id.into());
        self
    }

    /// Filter on `message_ids`.
    #[must_use]
    pub fn with_message_ids(mut self, message_ids: impl IntoIterator<Item = MessageId>) -> Self {
        self.0.ids = Some(message_ids.into_iter().collect());
        self
    }

    /// If true automatically convert simple queries to wildcarded versions, such as `test` to `*test*`.
    #[must_use]
    pub fn with_auto_wildcard(mut self, enabled: bool) -> Self {
        self.0.auto_wildcard = Some(enabled);
        self
    }

    /// Create the filter.
    #[must_use]
    pub fn build(self) -> MessageMetadataFilter {
        self.0
    }
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
impl crate::exports::proton_sqlite3::rusqlite::types::ToSql for MimeType {
    fn to_sql(
        &self,
    ) -> crate::exports::proton_sqlite3::rusqlite::Result<
        crate::exports::proton_sqlite3::rusqlite::types::ToSqlOutput<'_>,
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
impl crate::exports::proton_sqlite3::rusqlite::types::FromSql for MimeType {
    fn column_result(
        value: crate::exports::proton_sqlite3::rusqlite::types::ValueRef<'_>,
    ) -> crate::exports::proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
        let value = value.as_str()?;
        Ok(match value {
            "text/plain" => MimeType::TextPlain,
            "text/html" => MimeType::TextHTML,
            "multipart/mixed" => MimeType::MultipartMixed,
            "multipart/related" => MimeType::MultipartRelated,
            "message/rfc822" => MimeType::MessageRFC822,
            _ => {
                return Err(
                    crate::exports::proton_sqlite3::rusqlite::types::FromSqlError::Other(
                        format!("invalid mime type value:{value}").into(),
                    ),
                )
            }
        })
    }
}
