use crate::{LocalConversationWithContext, LocalLabelId, MailSqliteConnectionImpl};
use proton_api_mail::domain::{LabelId, SysLabelId};
use proton_sqlite3::{LiveQuery, ObservableQuery, SqliteConnection};
use std::ops::Deref;

#[derive(Clone, Default)]
pub struct ConversationQuery(Option<LocalLabelId>);

impl ConversationQuery {
    pub fn new() -> Self {
        Self(None)
    }
    pub fn with_label(label_id: LocalLabelId) -> Self {
        Self(Some(label_id))
    }
}

impl ObservableQuery for ConversationQuery {
    type Output = Vec<LocalConversationWithContext>;

    fn debug_name(&self) -> &'static str {
        "MailboxViewQuery"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "conversations".to_string(),
            "conversation_labels".to_string(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.deref());
        let label_id = if let Some(id) = self.0 {
            id
        } else {
            conn.resolve_remote_label_ids(std::iter::once(&LabelId::from(SysLabelId::INBOX)))?[0]
        };

        let conversations = conn.get_conversations_with_context(label_id, 25)?;
        Ok(conversations)
    }
}

pub type ConversationsLiveQuery = LiveQuery<ConversationQuery>;
