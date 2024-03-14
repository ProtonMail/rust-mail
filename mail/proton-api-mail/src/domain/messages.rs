use crate::domain::{AddressId, ConversationId, LabelId};
use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde::{self, Deserialize, Serialize, Serializer};

proton_api_core::utils::string_id!(MessageId);
proton_api_core::utils::string_id!(ExternalId);
proton_api_core::utils::string_id!(AttachmentId);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Default)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MessageAddress {
    //TODO: Proper email parsing
    pub address: String,
    pub name: String,
    #[serde(default)]
    pub is_proton: ProtonBoolean,
    #[serde(default)]
    pub display_sender_image: ProtonBoolean,
    #[serde(default)]
    pub is_simple_login: ProtonBoolean,
    pub bimi_selector: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
    pub unread: ProtonBoolean,
    pub is_replied: ProtonBoolean,
    pub is_replied_all: ProtonBoolean,
    pub is_forwarded: ProtonBoolean,
    pub expiration_time: u64,
    pub num_attachments: u32,
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(crate = "self::serde", rename_all = "snake_case")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Disposition {
    Inline,
    Attachment,
}

#[cfg(feature = "sql")]
use proton_api_core::exports::proton_sqlite3::rusqlite;

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
pub struct Message {
    #[serde(rename = "ID")]
    pub id: MessageId,
    #[serde(rename = "ConversationID")]
    pub conversation_id: ConversationId,
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
    pub order: u64,
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
    #[serde(rename = "CCList")]
    //TODO: Doesn't have to be default, but fails with GPA otherwise
    pub cc_list: Option<Vec<MessageAddress>>,
    #[serde(rename = "BCCList")]
    //TODO: Doesn't have to be default, but fails with GPA otherwise
    pub bcc_list: Option<Vec<MessageAddress>>,
    #[serde(default)]
    pub reply_tos: Vec<MessageAddress>,
    pub flags: u64,
    pub time: u64,
    pub size: u64,
    pub unread: ProtonBoolean,
    pub is_replied: ProtonBoolean,
    pub is_replied_all: ProtonBoolean,
    pub is_forwarded: ProtonBoolean,

    pub num_attachments: u32,

    pub header: Option<String>,
    //TODO:
    //pub parsed_headers: Headers,
    pub body: Option<String>,
    #[serde(rename = "MIMEType")]
    pub mime_type: Option<MimeType>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
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
    pub key_packets: Option<String>,
    pub signature: Option<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Copy)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageAttachmentInfo {
    #[serde(default)]
    pub attachment: u32,
    #[serde(default)]
    pub inline: u32,
}

#[derive(Debug, Copy, Clone)]
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

#[derive(Debug, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MessageMetadataFilter {
    #[serde(rename = "ID")]
    ids: Option<Vec<MessageId>>,
    subject: Option<String>,
    #[serde(rename = "AddressID")]
    address_id: Option<AddressId>,
    #[serde(rename = "LabelID")]
    label_id: Option<Vec<LabelId>>,
    #[serde(rename = "ExternalID")]
    external_id: Option<ExternalId>,
    #[serde(rename = "EndID")]
    end_id: Option<MessageId>,
    desc: ProtonBoolean,
    sort: Option<MessageMetadataSortMode>,
    #[serde(rename = "ConversationID")]
    conversation_id: Option<ConversationId>,
    page: usize,
    page_size: usize,
}

impl MessageMetadataFilter {
    fn new(page_number: usize, page_size: usize) -> Self {
        Self {
            ids: None,
            subject: None,
            address_id: None,
            external_id: None,
            end_id: None,
            label_id: None,
            sort: None,
            desc: ProtonBoolean::False,
            conversation_id: None,
            page_size,
            page: page_number,
        }
    }
}

#[derive(Debug)]
pub struct MessageMetadataFilterBuilder(MessageMetadataFilter);

impl MessageMetadataFilterBuilder {
    pub fn new(page_number: usize, page_size: usize) -> Self {
        Self(MessageMetadataFilter::new(page_number, page_size))
    }
    pub fn with_message_ids(mut self, ids: impl IntoIterator<Item = MessageId>) -> Self {
        self.0.ids = Some(ids.into_iter().collect());
        self
    }

    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.0.subject = Some(subject.into());
        self
    }

    pub fn with_external_id(mut self, id: impl Into<ExternalId>) -> Self {
        self.0.external_id = Some(id.into());
        self
    }

    pub fn with_address_id(mut self, id: impl Into<AddressId>) -> Self {
        self.0.address_id = Some(id.into());
        self
    }

    pub fn with_conversation_id(mut self, id: impl Into<ConversationId>) -> Self {
        self.0.conversation_id = Some(id.into());
        self
    }

    pub fn with_label_id(mut self, id: impl Into<LabelId>) -> Self {
        match &mut self.0.label_id {
            None => {
                self.0.label_id = Some(vec![id.into()]);
            }
            Some(v) => {
                v.push(id.into());
            }
        };
        self
    }

    pub fn with_end_id(mut self, id: impl Into<MessageId>) -> Self {
        self.0.end_id = Some(id.into());
        self
    }

    pub fn descending(mut self) -> Self {
        self.0.desc = ProtonBoolean::True;
        self
    }

    pub fn with_sort_mode(mut self, mode: MessageMetadataSortMode) -> Self {
        self.0.sort = Some(mode);
        self
    }

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
