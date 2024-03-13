use proton_mail_common::proton_api_mail::domain::MailSettings;
use crate::mail::{MailContextResult, MailUserContext};

#[uniffi::export]
impl MailUserContext {
    pub fn mail_settings(&self) -> MailContextResult<MailSettings> {
        let settings = self.ctx.mail_settings()?;
        Ok(settings)
    }
}
