use crate::domain::{
    AttachmentMetadata, ExternalId, LabelId, MessageAddress, MessageAttachmentInfo,
    MessageMetadataSortMode,
};
use crate::MAX_PAGE_ELEMENT_COUNT;
use proton_api_core::domain::AddressId;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use std::collections::HashMap;

proton_api_core::utils::string_id!(ConversationId);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationCount {
    #[serde(rename = "LabelID")]
    pub label_id: LabelId,
    pub total: u64,
    pub unread: u64,
}

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

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ConversationLabels {
    #[serde(rename = "ID")]
    pub id: LabelId,
    pub context_num_unread: u64,
    pub context_num_messages: u64,
    pub context_time: u64,
    pub context_size: u64,
    pub context_num_attachments: u64,
    pub context_expiration_time: u64,
    pub context_snooze_time: u64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
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

impl Conversation {
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.labels.iter().any(|l| l.id == *LabelId::starred())
    }
}
/// Parameters to filter/search conversations with a given criteria.
#[derive(Debug)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ConversationFilter {
    /// Conversation ids to filter on.
    pub ids: Option<Vec<ConversationId>>,
    /// Keyword search of Subject field.
    pub subject: Option<String>,
    /// Keyword search of From field.
    pub from: Option<String>,
    /// Keyword search of To, CC and BCC fields.
    pub recipients: Option<Vec<String>>,
    /// Keyword search of To, CC, BCC, From and Subject fields
    pub keyword: Option<String>,
    /// Address id to filter on.
    pub address_id: Option<AddressId>,
    /// Label id to filter on.
    pub label_id: Option<LabelId>,
    /// External id to filter on.
    pub external_id: Option<ExternalId>,
    /// Return only conversations older, in creation time (NOT timestamp), than `end_id` if timestamp = `end`
    pub end_id: Option<ConversationId>,
    /// Return only conversations newer, in creation time (NOT timestamp), than `begin_id` if timestamp = `begin`
    pub begin_id: Option<ConversationId>,
    /// UNIX timestamp to filter conversations earlier than timestamp
    pub begin: Option<u64>,
    /// UNIX timestamp to filter conversations later than timestamp
    pub end: Option<u64>,
    /// If true, return results in descending order rather than ascending.
    pub desc: Option<bool>,
    /// If true, only return conversations which have attachments. If false, only return
    /// conversations which have no attachments.
    pub attachments: Option<bool>,
    /// If true, only return conversations which have unread messages. If false only return
    /// conversations which have all messages read.
    pub unread: Option<bool>,
    /// Sort the results by one of the sorting modes.
    pub sort: Option<MessageMetadataSortMode>,
    /// The number of conversations to return.
    pub limit: Option<u64>,
    /// If true automatically convert simple queries to wildcarded versions, such as `test` to `*test*`.
    pub auto_wildcard: Option<bool>,
    /// Page index
    pub page: u64,
    /// Number of elements per page.
    pub page_size: u64,
}

impl ConversationFilter {
    fn new(page_index: usize, page_size: usize) -> Self {
        Self {
            ids: None,
            subject: None,
            from: None,
            recipients: None,
            keyword: None,
            address_id: None,
            external_id: None,
            end_id: None,
            begin_id: None,
            begin: None,
            end: None,
            desc: None,
            attachments: None,
            label_id: None,
            sort: None,
            page_size: page_size.min(MAX_PAGE_ELEMENT_COUNT) as u64,
            page: page_index as u64,
            unread: None,
            limit: None,
            auto_wildcard: None,
        }
    }
}

/// Builder for [`ConversationFilter`].
#[derive(Debug)]
pub struct ConversationFilterBuilder(ConversationFilter);

impl ConversationFilterBuilder {
    /// Create a new builder for `page_index` and with a `page_size` number of elements.
    #[must_use]
    pub fn new(page_index: usize, page_size: usize) -> Self {
        Self(ConversationFilter::new(page_index, page_size))
    }

    /// Conversation ids to filter on.
    #[must_use]
    pub fn with_conversation_ids(mut self, ids: impl IntoIterator<Item = ConversationId>) -> Self {
        self.0.ids = Some(ids.into_iter().collect());
        self
    }

    /// Keyword search of Subject field.
    #[must_use]
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.0.subject = Some(subject.into());
        self
    }

    /// Keyword search of From field.
    #[must_use]
    pub fn with_from(mut self, from: impl Into<String>) -> Self {
        self.0.from = Some(from.into());
        self
    }

    /// Keyword search of To, CC and BCC fiels.
    #[must_use]
    pub fn with_recipients(mut self, recipients: impl IntoIterator<Item = String>) -> Self {
        self.0.recipients = Some(recipients.into_iter().collect());
        self
    }

    /// Keyword search of To, CC, BBC, From and Subject fields.
    #[must_use]
    pub fn with_keyword(mut self, keyword: impl Into<String>) -> Self {
        self.0.keyword = Some(keyword.into());
        self
    }

    /// External id to filter on.
    #[must_use]
    pub fn with_external_id(mut self, id: impl Into<ExternalId>) -> Self {
        self.0.external_id = Some(id.into());
        self
    }

    /// Address id to filter on.
    #[must_use]
    pub fn with_address_id(mut self, id: impl Into<AddressId>) -> Self {
        self.0.address_id = Some(id.into());
        self
    }

    /// Label id to filter on.
    #[must_use]
    pub fn with_label_id(mut self, id: impl Into<LabelId>) -> Self {
        self.0.label_id = Some(id.into());
        self
    }

    /// Return only conversations older, in creation time (NOT timestamp), than `end_id` if timestamp = `end`
    #[must_use]
    pub fn with_end_id(mut self, end_id: impl Into<ConversationId>) -> Self {
        self.0.end_id = Some(end_id.into());
        self
    }

    /// Return only conversations new, in creation time (NOT timestamp), than `begin_id` if timestamp = `begin`
    #[must_use]
    pub fn with_begin_id(mut self, begin_id: impl Into<ConversationId>) -> Self {
        self.0.begin_id = Some(begin_id.into());
        self
    }

    /// UNIX timestamp to filter conversations earlier than `timestamp`.
    #[must_use]
    pub fn with_begin(mut self, timestamp: u64) -> Self {
        self.0.begin = Some(timestamp);
        self
    }

    /// UNIX timestamp to filter conversations earlier than `timestamp`.
    #[must_use]
    pub fn with_end(mut self, timestamp: u64) -> Self {
        self.0.end = Some(timestamp);
        self
    }

    /// Order results in descending order.
    #[must_use]
    pub fn descending(mut self) -> Self {
        self.0.desc = Some(true);
        self
    }

    /// Order results in ascending order.
    #[must_use]
    pub fn ascending(mut self) -> Self {
        self.0.desc = Some(false);
        self
    }

    /// Only return conversations which have attachments.
    #[must_use]
    pub fn with_attachments(mut self) -> Self {
        self.0.attachments = Some(true);
        self
    }

    /// Only return conversations which have no attachments.
    #[must_use]
    pub fn without_attachments(mut self) -> Self {
        self.0.attachments = Some(false);
        self
    }

    /// Only return conversations which have unread messages.
    #[must_use]
    pub fn with_unread(mut self) -> Self {
        self.0.unread = Some(true);
        self
    }

    /// Only return conversations which have read messages.
    #[must_use]
    pub fn with_read(mut self) -> Self {
        self.0.unread = Some(false);
        self
    }

    /// Sort the results according to `mode`
    #[must_use]
    pub fn with_sort_mode(mut self, mode: MessageMetadataSortMode) -> Self {
        self.0.sort = Some(mode);
        self
    }

    /// Limit the results up to `limit` conversations.
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.0.limit = Some(limit as u64);
        self
    }

    /// If true automatically convert simple queries to wildcarded versions, such as `test` to `*test*`.
    #[must_use]
    pub fn with_auto_wildcard(mut self, enabled: bool) -> Self {
        self.0.auto_wildcard = Some(enabled);
        self
    }

    #[must_use]
    pub fn build(self) -> ConversationFilter {
        self.0
    }
}
