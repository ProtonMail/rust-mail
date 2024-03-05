use crate::{DBError, LocalConversationWithContext, LocalLabelId, MailSqliteConnectionImpl};
use proton_api_mail::domain::{LabelId, SysLabelId};
use proton_sqlite3::{LiveQuery, ObservableQuery, SqliteConnection};
use std::ops::Deref;

#[derive(Clone, Default)]
pub struct ConversationQuery {
    label_id: Option<LocalLabelId>,
    limit: usize,
}

impl ConversationQuery {
    pub fn new(limit: usize) -> Self {
        debug_assert!(limit > 0);
        Self {
            limit,
            label_id: None,
        }
    }
    pub fn with_label(limit: usize, label_id: LocalLabelId) -> Self {
        debug_assert!(limit > 0);
        Self {
            limit,
            label_id: Some(label_id),
        }
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
        let label_id = if let Some(id) = self.label_id {
            id
        } else {
            let result =
                conn.resolve_remote_label_ids(std::iter::once(&LabelId::from(SysLabelId::INBOX)))?;
            if result.is_empty() {
                return Err(DBError::QueryReturnedNoRows);
            }
            result[0]
        };

        let conversations = conn.get_conversations_with_context(label_id, self.limit)?;
        Ok(conversations)
    }
}

pub type ConversationsLiveQuery = LiveQuery<ConversationQuery>;
