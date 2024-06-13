use crate::mail::mailbox::{
    FFIObservableConversationsQueryBuilder, Observable, SharedLiveQueryBuilder,
    DEFAULT_CONVERSATION_COUNT,
};
use crate::mail::{
    Mailbox, MailboxConversationLiveQuery, MailboxError, MailboxLiveQueryUpdatedCallback,
};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::InProcessTrackerService;
use proton_mail_common::db::proton_sqlite3::SharedLive;
use proton_mail_common::db::{ConversationMessagesQuery, LocalConversationId, LocalLabelId};
use proton_mail_common::proton_api_mail::domain::LabelId;
use proton_mail_common::MailboxObservableQueryBuilder;
use std::sync::Arc;
use uniffi::deps::anyhow::anyhow;

#[uniffi::export]
impl Mailbox {
    /// Create a live query for conversations for the currently selected label.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Conversations`].
    pub fn new_conversation_live_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Result<Arc<MailboxConversationLiveQuery>, MailboxError> {
        let limit = usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT);
        let builder = FFIObservableConversationsQueryBuilder(cb);
        Ok(self.mbox.new_conversation_query(builder, limit)?)
    }

    /// Delete/Destroy the given conversations for the current mailbox.
    pub fn delete_conversations(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .delete_conversations(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Mark the given conversations as read.
    pub fn mark_conversations_read(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .mark_conversations_read(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Mark the given conversations as unread.
    pub fn mark_conversations_unread(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .mark_conversations_unread(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Move the given conversations from the current mailbox.
    ///
    /// Move the conversations with `ids` from the current mailbox to the label with id `label_id`.
    /// If the current mailbox is not a folder, the conversation will not be moved.
    /// To retrieve the list of movable folders use the
    /// [`crate::mail::MailUserSession::movable_folders()`] method.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn move_conversations(&self, label_id: u64, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox.move_conversations(
            LocalLabelId::new(label_id),
            ids.into_iter().map(LocalConversationId::from),
        )?;
        Ok(())
    }

    /// Move the given conversations from the current mailbox.
    ///
    /// Move the conversations with `ids` from the current mailbox to the label with remote id `label_id`.
    /// If the current mailbox is not a folder, the conversation will not be moved.
    /// To retrieve the list of movable folders use the
    /// [`crate::mail::MailUserSession::movable_folders()`] method.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn move_conversations_with_remote_id(
        &self,
        label_id: &LabelId,
        ids: Vec<u64>,
    ) -> Result<(), MailboxError> {
        self.mbox.move_conversations_with_remote_id(
            label_id,
            ids.into_iter().map(LocalConversationId::from),
        )?;
        Ok(())
    }

    /// Label the given conversations with the given label id.
    ///
    /// To retrieve the list of applicable labels use the
    /// [`crate::mail::MailUserSession::applicable_labels()`] method.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn label_conversations(&self, label_id: u64, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox.label_conversations(
            LocalLabelId::new(label_id),
            ids.into_iter().map(LocalConversationId::from),
        )?;
        Ok(())
    }

    /// Unlabel the given conversations with the given label id.
    ///
    /// To retrieve the list of applicable labels use the
    /// [`crate::mail::MailUserSession::applicable_labels()`] method.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn unlabel_conversations(&self, label_id: u64, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox.unlabel_conversations(
            LocalLabelId::new(label_id),
            ids.into_iter().map(LocalConversationId::from),
        )?;
        Ok(())
    }

    /// Star the given conversations.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn star_conversations(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .star_conversations(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Unstar the given conversations.
    ///
    /// # Errors
    /// Returns error if the action fails.
    pub fn unstar_conversations(&self, ids: Vec<u64>) -> Result<(), MailboxError> {
        self.mbox
            .unstar_conversations(ids.into_iter().map(LocalConversationId::from))?;
        Ok(())
    }

    /// Create a new live query for a conversation with `id` 's messages and return the first id of
    /// the  message that should be displayed to the user.
    ///
    /// If this is the first time it is called for this conversation, the messages will
    /// be retrieved from the server.
    ///
    /// # Errors
    /// Returns error if the db queries failed, the network request failed or the conversation
    /// has no messages.
    pub async fn new_conversation_messages_live_query(
        &self,
        id: u64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Result<ConversationMessagesLiveQueryResult, MailboxError> {
        let mbox = self.mbox.clone();
        let id = LocalConversationId::from(id);
        let builder = FFIObservableConversationMessagesQueryBuilder(cb);
        let query = mbox.new_conversation_message_query(builder, id).await?;

        let id = match query.value().as_ref() {
            Err(e) => {
                return Err(MailboxError::Other(anyhow!("Live query failed: {e}")));
            }
            // If no unread message is returned, use last message id.
            Ok(v) => match mbox.first_unread_message_in_conversation(v.as_slice())? {
                Some(id) => Some(id),
                None => v.last().map(|v| v.id),
            },
        }
        .ok_or(MailboxError::ConversationHasNoMessages(id))?;

        Ok(ConversationMessagesLiveQueryResult {
            message_id_to_open: id.value(),
            query,
        })
    }
}

/// Result type for [`Mailbox::new_conversation_messages_live_query`],
#[derive(uniffi::Record)]
pub struct ConversationMessagesLiveQueryResult {
    /// Id of the message that should be opened and displayed to the user.
    pub message_id_to_open: u64,
    /// Live query instance.
    pub query: Arc<MailboxConversationMessagesLiveQuery>,
}

new_live_query!(
    MailboxConversationMessagesLiveQuery,
    ConversationMessagesQuery
);

struct FFIObservableConversationMessagesQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<ConversationMessagesQuery>
    for FFIObservableConversationMessagesQueryBuilder
{
    type Output = Arc<MailboxConversationMessagesLiveQuery>;

    fn build(
        self,
        tracker: InProcessTrackerService,
        query: ConversationMessagesQuery,
    ) -> Self::Output {
        MailboxConversationMessagesLiveQuery::new_foreground(tracker, query, self.0)
    }
}
