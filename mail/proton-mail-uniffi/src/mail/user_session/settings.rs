use crate::mail::{MailSessionResult, MailUserSession};
use proton_mail_common::proton_api_mail::domain::MailSettings;

#[uniffi::export]
impl MailUserSession {
    /// Returns the user's mail settings.
    pub fn mail_settings(&self) -> MailSessionResult<MailSettings> {
        let settings = self.ctx.mail_settings()?;
        Ok(settings)
    }
}
