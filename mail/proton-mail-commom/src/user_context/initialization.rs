use crate::{MailContextError, MailUserContext};
use proton_api_mail::domain::LabelId;
use proton_api_mail::proton_api_core::exports::tracing::{self, error, trace, Level};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    User,
    Addresses,
    Events,
    Labels,
    Counters,
    Conversation,
    Finished,
}
pub trait MailUserContextInitializationCallback: Send + Sync + 'static {
    fn on_stage(&self, stage: MailUserContextLoadingStage);
    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError);
}

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self, cb), fields(user_id=?self.user_id()))]
    pub fn initialize(
        &self,
        label_id: LabelId,
        cb: Box<dyn MailUserContextInitializationCallback>,
    ) {
        let ctx = self.clone();
        self.mail_context().async_runtime().spawn(async move {
            trace!("Syncing User settings");
            cb.on_stage(MailUserContextLoadingStage::User);
            if let Err(e) = ctx.inner.user_context.sync_user_and_settings().await {
                error!("Failed to sync user settings: {e}");
                cb.on_stage_err(MailUserContextLoadingStage::User, e.into());
                return;
            }

            trace!("Syncing event id");
            cb.on_stage(MailUserContextLoadingStage::Events);
            if let Err(e) = ctx.inner.event_loop.initialize(&ctx, &ctx).await {
                error!("Failed to sync event id:{e}");
                cb.on_stage_err(MailUserContextLoadingStage::Events, e.into());
                return;
            }

            trace!("Syncing Addresses");
            cb.on_stage(MailUserContextLoadingStage::Addresses);
            if let Err(e) = ctx.sync_addresses().await {
                error!("Failed to sync addresses :{e}");
                cb.on_stage_err(MailUserContextLoadingStage::Addresses, e);
                return;
            }

            // load labels
            trace!("Syncing labels");
            cb.on_stage(MailUserContextLoadingStage::Labels);
            if let Err(e) = ctx.sync_labels().await {
                error!("Failed to sync labels: {e}");
                cb.on_stage_err(MailUserContextLoadingStage::Labels, e);
                return;
            }

            // load conversation counters
            trace!("Syncing conversation and message counts");
            cb.on_stage(MailUserContextLoadingStage::Counters);
            if let Err(e) = ctx.sync_conversation_and_message_counts().await {
                error!("Failed to sync conversation and messages counter: {e}");
                cb.on_stage_err(MailUserContextLoadingStage::Counters, e);
            }

            // load inbox conversations
            trace!("Syncing Inbox conversations");
            cb.on_stage(MailUserContextLoadingStage::Conversation);
            if let Err(e) = ctx.sync_first_conversation_page(label_id, 50).await {
                error!("Failed to sync Inbox conversation: {e}");
                cb.on_stage_err(MailUserContextLoadingStage::Conversation, e);
                return;
            }

            trace!("Syncing Complete");
            cb.on_stage(MailUserContextLoadingStage::Finished);
        });
    }
}
