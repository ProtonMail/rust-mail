use crate::db::MailSqliteConnectionImpl;
use proton_api_mail::domain::MailSettings;
use proton_sqlite3::{
    InProcessTrackerService, LiveQueryUpdated, Observable, SharedLive, SharedLiveQueryBuilder,
    SqliteConnection,
};

/// Mail Settings Live Query.
pub type MailSettingsLiveQuery = SharedLive<MailSettingsObservable>;

/// Create a new live query for the mail settings.
pub fn new_mail_settings_live_query(
    tracker: InProcessTrackerService,
    cb: Option<Box<dyn LiveQueryUpdated>>,
) -> MailSettingsLiveQuery {
    let mut query = SharedLiveQueryBuilder::new(tracker).with_foreground_initializer();
    if let Some(cb) = cb {
        query = query.with_dyn_callback(cb);
    }
    query.build(MailSettingsObservable {})
}

/// Observer for the user's mail settings.
#[derive(Clone)]
pub struct MailSettingsObservable {}

impl Observable for MailSettingsObservable {
    type Output = MailSettings;

    fn debug_name(&self) -> &'static str {
        "mail_settings_observer"
    }

    fn tables(&self) -> Vec<String> {
        vec!["mail_settings".to_owned()]
    }

    fn execute(
        &self,
        connection: &SqliteConnection,
    ) -> proton_sqlite3::rusqlite::Result<Self::Output> {
        let conn = MailSqliteConnectionImpl::new(connection.rusqlite_connection());
        conn.mail_settings()
    }
}
