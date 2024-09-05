use crate::mail::{MailSessionError, MailSessionResult, MailUserSession};
use crate::uniffi_async;

#[uniffi::export]
impl MailUserSession {
    /// Initialize the user context. Should be called at least once.
    ///
    /// *NOTE*: You should not create any [`crate::mail::Mailbox`] types until this initialization has
    /// completed.
    pub async fn initialize(
        &self,
        cb: Box<dyn MailUserSessionInitializationCallback>,
    ) -> MailSessionResult<()> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            let cb = Box::new(FFIMailUserInitializationCallback::from(cb));
            let cb_ref = cb.as_ref();
            if let Err((_, e)) = ctx.initialize_async(cb_ref).await {
                return Err(MailSessionError::from(e));
            }
            Ok(())
        })
        .await?;
        Ok(())
    }
}

/// Stage of the initialization that is currently being handled.
#[derive(Debug, Copy, Clone, Eq, PartialEq, uniffi::Enum)]
pub enum MailUserSessionInitializationStage {
    User,
    MailSettings,
    Addresses,
    Events,
    Labels,
    Counters,
    Finished,
}

/// Callback for initialization progress.
#[uniffi::export(callback_interface)]
pub trait MailUserSessionInitializationCallback: Send + Sync {
    /// Called when a given initialization stage is entered.
    fn on_stage(&self, stage: MailUserSessionInitializationStage);
}
impl From<proton_mail_common::MailUserContextLoadingStage> for MailUserSessionInitializationStage {
    fn from(value: proton_mail_common::MailUserContextLoadingStage) -> Self {
        match value {
            proton_mail_common::MailUserContextLoadingStage::User => Self::User,
            proton_mail_common::MailUserContextLoadingStage::MailSettings => Self::MailSettings,
            proton_mail_common::MailUserContextLoadingStage::Addresses => Self::Addresses,
            proton_mail_common::MailUserContextLoadingStage::Events => Self::Events,
            proton_mail_common::MailUserContextLoadingStage::Labels => Self::Labels,
            proton_mail_common::MailUserContextLoadingStage::Counters => Self::Counters,
            proton_mail_common::MailUserContextLoadingStage::Finished => Self::Finished,
        }
    }
}

struct FFIMailUserInitializationCallback(Box<dyn MailUserSessionInitializationCallback>);
impl From<Box<dyn MailUserSessionInitializationCallback>> for FFIMailUserInitializationCallback {
    fn from(value: Box<dyn MailUserSessionInitializationCallback>) -> Self {
        Self(value)
    }
}

impl proton_mail_common::MailUserContextInitializationCallback
    for FFIMailUserInitializationCallback
{
    fn on_stage(&self, stage: proton_mail_common::MailUserContextLoadingStage) {
        self.0.on_stage(stage.into());
    }

    fn on_stage_err(
        &self,
        _: proton_mail_common::MailUserContextLoadingStage,
        _: proton_mail_common::MailContextError,
    ) {
        unreachable!()
    }
}
