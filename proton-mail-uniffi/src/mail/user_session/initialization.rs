use crate::mail::{MailSessionError, MailSessionResult, MailUserSession};
use proton_mail_common::exports::anyhow::anyhow;
use proton_mail_common::proton_api_mail::domain::LabelId;
use tokio::spawn;

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
        let cb = Box::new(FFIMailUserInitializationCallback::from(cb));
        let h = spawn(async move {
            let cb_ref = cb.as_ref();
            ctx.initialize_async(LabelId::inbox().clone(), cb_ref).await
        });
        if let Err((_, err)) = h
            .await
            .map_err(|e| MailSessionError::Other(anyhow!("Failed to join task: {e}")))?
        {
            return Err(err.into());
        }
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
