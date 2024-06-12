use crate::macros::LiveQueryError;
use crate::mail::MailUserSession;
use proton_mail_common::db::proton_sqlite3::LiveQueryUpdated;

/// Access the user's mail settings
#[derive(uniffi::Object)]
pub struct MailUserSettings {
    settings: proton_mail_common::settings::MailSettings,
}

#[uniffi::export]
impl MailUserSettings {
    /// Create a new mail settings instance.
    ///
    /// An optional `callback` can be provided to be signaled when
    /// the settings have been changed in the database.
    #[uniffi::constructor]
    pub fn new(session: &MailUserSession, callback: Option<Box<dyn MailSettingsUpdated>>) -> Self {
        Self {
            settings: proton_mail_common::settings::MailSettings::new(
                session.ctx(),
                callback.map(FFIMailsSettingsCallback::boxed),
            ),
        }
    }

    /// Returns the user's mail settings.
    pub fn value(
        &self,
    ) -> Result<proton_mail_common::proton_api_mail::domain::MailSettings, LiveQueryError> {
        match &*self.settings.value() {
            Ok(settings) => Ok(settings.clone()),
            Err(e) => Err(LiveQueryError::from_error(e)),
        }
    }
}

impl MailUserSettings {
    pub(super) fn settings(&self) -> &proton_mail_common::settings::MailSettings {
        &self.settings
    }
}

/// Callback interface to signal the mail settings have been updated.
#[uniffi::export(callback_interface)]
pub trait MailSettingsUpdated: Send + Sync {
    fn on_updated(&self);
}

/// Wrapper around [`MailSettingsUpdated`].
pub struct FFIMailsSettingsCallback(Box<dyn MailSettingsUpdated>);

impl FFIMailsSettingsCallback {
    #[must_use]
    pub fn boxed(cb: Box<dyn MailSettingsUpdated>) -> Box<dyn LiveQueryUpdated> {
        Box::new(Self(cb))
    }
}

impl LiveQueryUpdated for FFIMailsSettingsCallback {
    fn on_live_query_updated(&self) {
        self.0.on_updated();
    }
}
