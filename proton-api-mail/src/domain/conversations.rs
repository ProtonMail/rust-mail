use crate::domain::{ApiError, AttachmentMetadata, ExternalId, Label, LabelId, LabelType, MessageAddress, MessageAttachmentInfo, MessageId, MessageMetadata, MessageMetadataSortMode};
use proton_api_core::domain::AddressId;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use std::collections::HashMap;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;
use indoc::formatdoc;
use stash::exports::ToSql;
use tracing::debug;
use crate::{MailSession, MAX_PAGE_ELEMENT_COUNT};
use crate::requests::GetConversationsRequest;

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

#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("conversation_labels")]
pub struct ConversationLabels {
    #[IdField(autoincrement)]
    #[serde(skip)]
    pub local_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: LabelId,
    #[DbField]
    pub context_num_unread: u64,
    #[DbField]
    pub context_num_messages: u64,
    #[DbField]
    pub context_time: u64,
	#[DbField]
    pub context_size: u64,
	#[DbField]
    pub context_num_attachments: u64,
	#[DbField]
    pub context_expiration_time: u64,
    #[DbField]
    pub context_snooze_time: u64,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("conversations")]
pub struct Conversation {
    #[IdField(autoincrement)]
    #[serde(skip)]
    pub local_id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: Option<ConversationId>,
    #[DbField]
    pub order: u64,
    #[DbField]
    pub subject: String,
    #[serde(default)]
    pub senders: Vec<MessageAddress>,
    #[serde(default)]
    pub recipients: Vec<MessageAddress>,
    #[DbField]
    pub num_messages: u64,
    #[DbField]
    pub num_unread: u64,
    #[DbField]
    pub num_attachments: u64,
    #[DbField]
    pub expiration_time: u64,
    #[DbField]
    pub size: u64,
    #[serde(default)]
    pub labels: Vec<ConversationLabels>,
    #[DbField]
    #[serde(default)]
    pub display_snooze_reminder: bool,
    #[serde(default)]
    pub attachments_metadata: Vec<AttachmentMetadata>,
    #[serde(default)]
    pub attachment_info: HashMap<String, MessageAttachmentInfo>,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

impl Conversation {
    async fn create_or_update_conversations(
        conversations: Vec<Conversation>,
        stash: &Stash,
    ) -> Result<Vec<u64>, ApiError> {
        let tx = stash.transaction().await?;
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            if let Some(existing) = Self::find("WHERE remote_id = ?", params![conv.remote_id.clone()], stash, None).await?.into_iter().next() {
                conv.local_id = existing.local_id;
                conv.row_id = existing.row_id;
                conv.stash = existing.stash;
                
                // Remove any labels that are no longer associated with this conversation.
                if !conv.labels.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(formatdoc!("
                        DELETE FROM
                            conversation_labels
                        WHERE
                            local_conversation_id = ?
                            AND local_label_id NOT IN (
                                SELECT local_id FROM labels WHERE remote_id IN ({})
                            )
                        ",
                        vec!["?"; conv.labels.len()].join(",")
                    ), vec![Box::new(conv.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>].into_iter().chain(conv.labels.iter().map(|label| Box::new(label.remote_id.clone()) as Box<dyn ToSql + Send>)).collect()).await?;
                } else {
                    tx.execute(formatdoc!("
                        DELETE FROM
                            conversation_labels
                        WHERE
                            local_conversation_id = ?
                        ",
                    ), params![conv.local_id]).await?;
                }

                // Remove any attachments that are no longer associated with this conversation.
                if !conv.attachments_metadata.is_empty() {
                    #[allow(trivial_casts)]
                    tx.execute(formatdoc!("
                        DELETE FROM
                            conversation_attachments
                        WHERE
                            local_conversation_id = ?
                            AND local_attachment_id NOT IN ({})
                        ",
                        vec!["?"; conv.attachments_metadata.len()].join(",")
                    ), vec![Box::new(conv.remote_id.clone().unwrap()) as Box<dyn ToSql + Send>].into_iter().chain(conv.attachments_metadata.iter().map(|attachment| Box::new(attachment.remote_id.clone()) as Box<dyn ToSql + Send>)).collect()).await?;
                } else {
                    tx.execute(formatdoc!("
                        DELETE FROM
                            conversation_attachments
                        WHERE
                            local_conversation_id = ?
                        ",
                    ), params![conv.local_id]).await?;
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
    
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.labels.iter().any(|l| l.remote_id == *LabelId::starred())
    }
    
    /// Retrieve the first unread message that should be displayed to the user
    /// from the conversation's `messages`.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    /// 
    // TODO: This should become an instance method later once all is stable.
    pub fn first_unread_message(
        label: &Label,
        messages: &[MessageMetadata],
    ) -> Option<MessageId> {
        if messages.is_empty() {
            return None;
        }
    
        if label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id.as_ref() == Some(LabelId::starred())
        {
            // last consecutive that is not a draft
            let mut last_unread = None;
    
            for msg in messages.iter().rev() {
                if msg.unread && !msg.flags.is_draft() {
                    last_unread = Some(msg.remote_id.clone());
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
            return Some(msg.remote_id.clone());
        }
    
        let mut last_unread = None;
    
        // last consecutive message that is not a draft or sent auto-reply
        for msg in iter {
            if msg.unread && !(msg.flags.is_draft() || msg.flags.is_sent_auto()) {
                last_unread = Some(msg.remote_id.clone());
            } else if last_unread.is_some() {
                break;
            }
        }
    
        last_unread
    }

    /// Synchronize the conversations and message counts for each label.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_conversation_and_message_counts(_stash: &Stash, _session: &MailSession) -> Result<(), ApiError> {
        // TODO
        // let conversation_counts = session.conversation_counts().await?;
        // let message_counts = session.message_counts().await?;
        // let tx = stash.transaction().await?;
        // tx.create_or_update_conversation_counts(conversation_counts.iter())?;
        // tx.create_or_update_message_counts(message_counts.iter())?;
        // tx.commit().await
        Ok(())
    }

    /// Synchronize the first `count` conversations of the label with `label_id`.
    ///
    /// # Errors
    /// Returns error if the API request failed or the data could not be written to the
    /// database.
    pub async fn sync_first_conversation_page(
        label_id: LabelId,
        count: usize,
        stash: &Stash,
        session: &MailSession,
    ) -> Result<(), ApiError> {
        let response = session.session()
            .execute_request(GetConversationsRequest::new(ConversationFilter {
                page: 0,
                page_size: count.max(MAX_PAGE_ELEMENT_COUNT) as u64,
                label_id: Some(label_id),
                desc: Some(true),
                ..Default::default()
            }))
            .await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        Self::create_or_update_conversations(response.conversations, stash).await?;
        Ok(())
    }
}
/// Parameters to filter/search conversations with a given criteria.
#[derive(Debug, Default)]
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
