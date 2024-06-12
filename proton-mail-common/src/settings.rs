use crate::db::{new_mail_settings_live_query, DBResult, MailSettingsLiveQuery};
use crate::MailUserContext;
use proton_sqlite3::LiveQueryUpdated;
use std::ops::Deref;

/// Access point for all mail related settings.
pub struct MailSettings {
    mail_settings: MailSettingsLiveQuery,
}

impl MailSettings {
    /// Create a new mail settings instance.
    ///
    /// An optional `callback` can be provided to be signaled when
    /// the settings have been changed in the database.
    pub fn new(ctx: &MailUserContext, callback: Option<Box<dyn LiveQueryUpdated>>) -> Self {
        let mail_settings = new_mail_settings_live_query(ctx.tracker_service().clone(), callback);
        Self { mail_settings }
    }

    /// Get the users mail settings.
    pub fn value(
        &self,
    ) -> impl Deref<Target = DBResult<proton_api_mail::domain::MailSettings>> + '_ {
        self.mail_settings.value()
    }

    /// Extract a value from the user's mail settings.
    pub fn with<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&DBResult<proton_api_mail::domain::MailSettings>) -> T,
    {
        f(self.mail_settings.value().deref())
    }
}
