use proton_sqlite3::{Observable, SqliteConnection};

use crate::db::CoreSqliteConnectionImpl;

use super::{LocalContact, LocalContactEmail};

/// Observable query which observers a limited number of contacts in a window.
#[derive(Clone)]
pub struct ObserveAllContacts {
    offset: u64,
    limit: u64,
}

impl ObserveAllContacts {
    #[must_use]
    pub fn new(offset: u64, limit: u64) -> Self {
        Self { offset, limit }
    }
}

impl Observable for ObserveAllContacts {
    type Output = Vec<LocalContact>;

    fn debug_name(&self) -> &'static str {
        "contact_view"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "contacts".to_owned(),
            "contact_emails".to_owned(),
            "contact_email_labels".to_owned(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = CoreSqliteConnectionImpl::new(connection.rusqlite_connection());
        conn.query_contacts(self.offset, self.limit)
    }
}

/// Observable query which observers a limited number of email contacts in a window.
#[derive(Clone)]
pub struct ObserveAllContactMails {
    offset: u64,
    limit: u64,
}

impl ObserveAllContactMails {
    #[must_use]
    pub fn new(offset: u64, limit: u64) -> Self {
        Self { offset, limit }
    }
}

impl Observable for ObserveAllContactMails {
    type Output = Vec<LocalContactEmail>;

    fn debug_name(&self) -> &'static str {
        "contact_emails_view"
    }

    fn tables(&self) -> Vec<String> {
        vec![
            "contact_emails".to_owned(),
            "contact_email_labels".to_owned(),
        ]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = CoreSqliteConnectionImpl::new(connection.rusqlite_connection());
        conn.query_contact_emails(self.offset, self.limit)
    }
}
