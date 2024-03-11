use crate::mail::MailContextError;
use proton_mail_common as pmc;
use proton_mail_common::exports::proton_event_loop::{EventLoopError as ELError, SubscriberError};
use proton_mail_common::exports::{anyhow, thiserror};
use proton_mail_common::proton_api_mail::domain::LabelId;
use proton_mail_common::proton_api_mail::proton_api_core::http::HttpRequestError;
use std::sync::Arc;

#[derive(uniffi::Object)]
pub struct MailUserContext {
    ctx: pmc::MailUserContext,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, uniffi::Enum)]
pub enum MailUserContextInitializationStage {
    User,
    Addresses,
    Events,
    Labels,
    Counters,
    Conversation,
    Finished,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum EventLoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(HttpRequestError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(String),
}

impl From<ELError> for EventLoopError {
    fn from(value: ELError) -> Self {
        match value {
            ELError::StoreRead(e) => EventLoopError::StoreRead(e),
            ELError::StoreWrite(e) => EventLoopError::StoreWrite(e),
            ELError::Provider(e) => EventLoopError::Provider(e),
            ELError::Subscriber(s, e) => EventLoopError::Subscriber(s, e),
            ELError::Other(s) => EventLoopError::Other(s),
        }
    }
}

/// Callback for initialization progress.
#[uniffi::export(callback_interface)]
pub trait MailUserContextInitializationCallback: Send + Sync {
    /// Called when a given initialization stage is entered.
    fn on_stage(&self, stage: MailUserContextInitializationStage);

    /// Called when a given initialization stage produces an error
    fn on_stage_err(&self, stage: MailUserContextInitializationStage, err: MailContextError);
}

impl MailUserContext {
    pub(crate) fn new(ctx: pmc::MailUserContext) -> Arc<Self> {
        Arc::new(Self { ctx })
    }
    pub(crate) fn ctx(&self) -> &pmc::MailUserContext {
        &self.ctx
    }
}

#[uniffi::export]
impl MailUserContext {
    /// Initialize the user context. Should be called at least once.
    pub fn initialize(&self, cb: Box<dyn MailUserContextInitializationCallback>) {
        let cb = Box::new(FFIMailUserInitializationCallback::from(cb));
        self.ctx.initialize(LabelId::inbox(), cb);
    }

    /// Poll Event loop and apply events.
    /// **NOTE**: This method should not be run on the main thread.
    pub fn poll_events(&self) -> Result<(), EventLoopError> {
        self.ctx.mail_context().async_runtime().block_on(async {
            self.ctx.poll_event_loop().await?;
            Ok(())
        })
    }
}
impl From<proton_mail_common::MailUserContextLoadingStage> for MailUserContextInitializationStage {
    fn from(value: proton_mail_common::MailUserContextLoadingStage) -> Self {
        match value {
            proton_mail_common::MailUserContextLoadingStage::User => Self::User,
            proton_mail_common::MailUserContextLoadingStage::Addresses => Self::Addresses,
            proton_mail_common::MailUserContextLoadingStage::Events => Self::Events,
            proton_mail_common::MailUserContextLoadingStage::Labels => Self::Labels,
            proton_mail_common::MailUserContextLoadingStage::Counters => Self::Counters,
            proton_mail_common::MailUserContextLoadingStage::Conversation => Self::Conversation,
            proton_mail_common::MailUserContextLoadingStage::Finished => Self::Finished,
        }
    }
}

struct FFIMailUserInitializationCallback(Box<dyn MailUserContextInitializationCallback>);
impl From<Box<dyn MailUserContextInitializationCallback>> for FFIMailUserInitializationCallback {
    fn from(value: Box<dyn MailUserContextInitializationCallback>) -> Self {
        Self(value)
    }
}

impl proton_mail_common::MailUserContextInitializationCallback
    for FFIMailUserInitializationCallback
{
    fn on_stage(&self, stage: proton_mail_common::MailUserContextLoadingStage) {
        self.0.on_stage(stage.into())
    }

    fn on_stage_err(
        &self,
        stage: proton_mail_common::MailUserContextLoadingStage,
        err: proton_mail_common::MailContextError,
    ) {
        self.0.on_stage_err(stage.into(), err.into())
    }
}
