use crate::{LocalLabel, MailSqliteConnectionImpl};
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
        conn.get_local_label_by_type_ordered(self.0)
    }
}

pub type LabelsByTypeLiveQuery = LiveQuery<LabelsByTypeQuery>;
