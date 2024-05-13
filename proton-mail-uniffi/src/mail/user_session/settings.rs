use crate::macros::LiveQueryError;
use crate::mail::MailUserSession;
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
