/// Callback interface to signal the mail settings have been updated.
#[uniffi::export(callback_interface)]
pub trait MailSettingsUpdated: Send + Sync {
    fn on_updated(&self);
}
