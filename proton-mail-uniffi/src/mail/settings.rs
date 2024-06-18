/// Access the user's mail settings
#[derive(uniffi::Object)]
pub struct MailUserSettings {}

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
