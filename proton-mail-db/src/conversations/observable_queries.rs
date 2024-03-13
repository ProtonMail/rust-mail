use crate::{LocalConversation, LocalLabelId, MailSqliteConnectionImpl};
use proton_sqlite3::{ObservableQuery, SqliteConnection};
use std::ops::Deref;

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

impl ObservableQuery for ConversationQuery {
    type Output = Vec<LocalConversation>;

    fn debug_name(&self) -> &'static str {
        "MailboxViewQuery"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "conversations".to_string(),
            "conversation_labels".to_string(),
            "labels".to_string(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.deref());
        let conversations = conn.get_conversations_with_context(self.label_id, self.limit)?;
        Ok(conversations)
    }
}
