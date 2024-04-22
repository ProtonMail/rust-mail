use crate::mail::mailbox::{FFIObservableConversationsQueryBuilder, DEFAULT_CONVERSATION_COUNT};
use crate::mail::{
    Mailbox, MailboxConversationLiveQuery, MailboxError, MailboxLiveQueryUpdatedCallback,
};
use proton_mail_common::db::{LocalConversationId, LocalLabelId};
use proton_mail_common::proton_api_mail::domain::LabelId;
use std::sync::Arc;

#[uniffi::export]
impl Mailbox {
    /// Create a live query for conversations for the currently selected label. If you
    /// change the mailbox label with `switch_label` you need to create a new instance.
    #[must_use]
    pub fn new_conversation_live_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Arc<MailboxConversationLiveQuery> {
        let limit = usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT);
        let builder = FFIObservableConversationsQueryBuilder(cb);
        self.mbox.new_conversation_query(builder, limit)
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
}
