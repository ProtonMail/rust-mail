use crate::db::{
    LocalConversation, LocalConversationCount, LocalConversationId, LocalLabelId,
    LocalMessageCount, LocalMessageMetadata, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::MailSettingsViewMode;
use proton_sqlite3::{Observable, SqliteConnection};

/// Observable query which observers a limited number of conversations in a label.
#[derive(Clone)]
pub struct ConversationQuery {
    label_id: LocalLabelId,
    limit: usize,
}

impl ConversationQuery {
    pub fn new(label_id: LocalLabelId, limit: usize) -> Self {
        debug_assert!(limit > 0);
        Self { limit, label_id }
    }
}

impl Observable for ConversationQuery {
    type Output = Vec<LocalConversation>;

    fn debug_name(&self) -> &'static str {
        "MailboxConversationView"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "conversations".to_owned(),
            "conversation_labels".to_owned(),
            "labels".to_owned(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        let conversations = conn.get_conversations_with_context(self.label_id, self.limit)?;
        Ok(conversations)
    }
}

/// Observable query which observers a limited number of messages in a label.
#[derive(Clone)]
pub struct MessageQuery {
    label_id: LocalLabelId,
    limit: usize,
}

impl MessageQuery {
    pub fn new(label_id: LocalLabelId, limit: usize) -> Self {
        debug_assert!(limit > 0);
        Self { limit, label_id }
    }
}

impl Observable for MessageQuery {
    type Output = Vec<LocalMessageMetadata>;

    fn debug_name(&self) -> &'static str {
        "MailboxMessageView"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "messages".to_owned(),
            "message_labels".to_owned(),
            "labels".to_owned(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        let conversations = conn.message_metadata_list(self.label_id, self.limit)?;
        Ok(conversations)
    }
}

/// Observable query which observers the messages of a conversation
#[derive(Clone)]
pub struct ConversationMessagesQuery {
    id: LocalConversationId,
}

impl ConversationMessagesQuery {
    pub fn new(id: LocalConversationId) -> Self {
        Self { id }
    }
}

impl Observable for ConversationMessagesQuery {
    type Output = Vec<LocalMessageMetadata>;

    fn debug_name(&self) -> &'static str {
        "ConversationMessages"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "messages".to_owned(),
            "message_labels".to_owned(),
            "labels".to_owned(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        let messages = conn.messages_metadata_for_conversation(self.id)?;
        Ok(messages)
    }
}

/// Observable query for label total and unread counts.
#[derive(Clone)]
pub struct LabelCountsQuery {
    id: LocalLabelId,
    view_mode: MailSettingsViewMode,
}

impl LabelCountsQuery {
    pub fn new(id: LocalLabelId, view_mode: MailSettingsViewMode) -> Self {
        Self { id, view_mode }
    }
}

/// Conversation/message statistic for a label.
#[derive(Default, Copy, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LabelItemCount {
    /// Number of unread messages or conversations.
    pub unread: u64,
    /// Number of messages or conversations.
    pub total: u64,
}

impl From<LocalConversationCount> for LabelItemCount {
    fn from(value: LocalConversationCount) -> Self {
        Self {
            unread: value.unread,
            total: value.total,
        }
    }
}

impl From<LocalMessageCount> for LabelItemCount {
    fn from(value: LocalMessageCount) -> Self {
        Self {
            unread: value.unread,
            total: value.total,
        }
    }
}

impl Observable for LabelCountsQuery {
    type Output = LabelItemCount;

    fn debug_name(&self) -> &'static str {
        "label_counts"
    }

    fn tables(&self) -> Vec<String> {
        match self.view_mode {
            MailSettingsViewMode::Conversations => {
                vec!["labels".to_owned(), "label_conversation_count".to_owned()]
            }
            MailSettingsViewMode::Messages => {
                vec!["labels".to_owned(), "label_message_count".to_owned()]
            }
        }
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        match self.view_mode {
            MailSettingsViewMode::Conversations => Ok(conn
                .conversation_count_for_label(self.id)?
                .map_or(LabelItemCount::default(), From::from)),
            MailSettingsViewMode::Messages => Ok(conn
                .message_count_for_label(self.id)?
                .map_or(LabelItemCount::default(), From::from)),
        }
    }
}
