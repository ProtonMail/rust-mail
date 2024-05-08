use crate::db::{
    LocalConversation, LocalConversationId, LocalLabelId, LocalMessageMetadata,
    MailSqliteConnectionImpl,
};
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
