use crate::db::{LocalLabel, LocalLabelWithCount, MailSqliteConnectionImpl};
use proton_api_mail::domain::LabelType;
use proton_sqlite3::{LiveQuery, ObservableQuery, SqliteConnection};

#[derive(Clone)]
pub struct LabelsByTypeQuery(LabelType);

impl LabelsByTypeQuery {
    pub fn new(label_type: LabelType) -> Self {
        Self(label_type)
    }
}

impl ObservableQuery for LabelsByTypeQuery {
    type Output = Vec<LocalLabel>;

    fn debug_name(&self) -> &'static str {
        "labels_by_type"
    }

    fn tables(&self) -> Vec<String> {
        vec!["labels".to_string()]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection);
        conn.label_by_type_ordered(self.0)
    }
}

pub type LabelsByTypeLiveQuery = LiveQuery<LabelsByTypeQuery>;

#[derive(Clone)]
pub struct LabelsByTypeQueryWithConversationCount(LabelType);

impl LabelsByTypeQueryWithConversationCount {
    pub fn new(label_type: LabelType) -> Self {
        Self(label_type)
    }
}

impl ObservableQuery for LabelsByTypeQueryWithConversationCount {
    type Output = Vec<LocalLabelWithCount>;

    fn debug_name(&self) -> &'static str {
        "labels_by_type"
    }

    fn tables(&self) -> Vec<String> {
        vec!["labels".to_string(), "label_conversation_count".to_string()]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection);
        conn.label_by_type_ordered_with_conversation_count(self.0)
    }
}
