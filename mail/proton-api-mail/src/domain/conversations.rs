use crate::domain::{
    AddressId, AttachmentMetadata, ExternalId, LabelId, MessageAddress, MessageAttachmentInfo,
    MessageMetadataSortMode,
};
use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use std::collections::HashMap;

proton_api_core::utils::string_id!(ConversationId);

#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationMetadata {
    #[serde(rename = "ID")]
    pub id: ConversationId,
    pub order: u64,
    pub subject: String,
    #[serde(default)]
    pub senders: Vec<MessageAddress>,
    #[serde(default)]
    pub recipients: Vec<MessageAddress>,
    pub num_messages: u64,
    pub num_unread: u64,
    pub num_attachments: u64,
    pub expiration_time: u64,
    pub size: u64,
    #[serde(default)]
    pub labels: Vec<ConversationLabels>,
    #[serde(default)]
    pub display_snooze_reminder: bool,
    pub context_num_messages: u64,
    pub context_num_unread: u64,
    pub context_num_attachments: u64,
    pub context_size: u64,
    pub context_time: u64,
    pub context_expiration_time: u64,
    pub address_id: AddressId,
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,
    #[serde(default)]
    pub attachment_info: HashMap<String, MessageAttachmentInfo>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationLabels {
    #[serde(rename = "ID")]
    pub id: LabelId,
    pub context_num_unread: u64,
    pub context_num_messages: u64,
    pub context_time: u64,
    pub context_size: u64,
    pub context_num_attachments: u64,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct Conversation {
    #[serde(rename = "ID")]
    pub id: ConversationId,
    pub order: u64,
    pub subject: String,
    #[serde(default)]
    pub senders: Vec<MessageAddress>,
    #[serde(default)]
    pub recipients: Vec<MessageAddress>,
    pub num_messages: u64,
    pub num_unread: u64,
    pub num_attachments: u64,
    pub expiration_time: u64,
    pub size: u64,
    #[serde(default)]
    pub labels: Vec<ConversationLabels>,
    #[serde(default)]
    pub display_snooze_reminder: bool,
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,
    #[serde(default)]
    pub attachment_info: HashMap<String, MessageAttachmentInfo>,
}
#[derive(Debug, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationFilter {
    #[serde(rename = "ID")]
    pub ids: Option<Vec<ConversationId>>,
    pub subject: Option<String>,
    #[serde(rename = "AddressID")]
    pub address_id: Option<AddressId>,
    #[serde(rename = "LabelID")]
    pub label_id: Option<Vec<LabelId>>,
    #[serde(rename = "ExternalID")]
    pub external_id: Option<ExternalId>,
    #[serde(rename = "EndID")]
    pub end_id: Option<ConversationId>,
    pub desc: ProtonBoolean,
    pub sort: Option<MessageMetadataSortMode>,
    pub page: usize,
    pub page_size: usize,
}

impl ConversationFilter {
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
            page_size,
            page: page_number,
        }
    }
}

#[derive(Debug)]
pub struct ConversationFilterBuilder(ConversationFilter);

impl ConversationFilterBuilder {
    pub fn new(page_number: usize, page_size: usize) -> Self {
        Self(ConversationFilter::new(page_number, page_size))
    }
    pub fn with_message_ids(mut self, ids: impl IntoIterator<Item = ConversationId>) -> Self {
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

    pub fn with_end_id(mut self, id: impl Into<ConversationId>) -> Self {
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

    pub fn build(self) -> ConversationFilter {
        self.0
    }
}
