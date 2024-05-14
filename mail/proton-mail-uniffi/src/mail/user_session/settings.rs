use crate::macros::LiveQueryError;
use crate::mail::MailUserSession;
use proton_mail_common::db::proton_sqlite3::LiveQueryUpdated;
use proton_mail_common::proton_api_mail::domain::MailSettings;

#[uniffi::export]
impl MailUserSession {
    /// Returns the user's mail settings.
    pub fn mail_settings(&self) -> Result<MailSettings, LiveQueryError> {
        match &*self.ctx.mail_settings() {
            Ok(settings) => Ok(settings.clone()),
            Err(e) => Err(LiveQueryError::from_error(e)),
        }
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
    pub fn new(cb: Box<dyn MailSettingsUpdated>) -> Self {
        Self(cb)
    }

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
