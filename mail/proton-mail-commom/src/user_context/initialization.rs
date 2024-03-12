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
            if let Err((stage, err)) = ctx.initialize_async(label_id, cb.as_ref()).await {
                cb.on_stage_err(stage, err);
            }
        });
    }

    #[tracing::instrument(level = Level::DEBUG, skip(self, cb), fields(user_id=?self.user_id()))]
    pub async fn initialize_async(
        &self,
        label_id: LabelId,
        cb: &dyn MailUserContextInitializationCallback,
    ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
        let ctx = self;
        trace!("Syncing User settings");
        cb.on_stage(MailUserContextLoadingStage::User);
        if let Err(e) = ctx.inner.user_context.sync_user_and_settings().await {
            error!("Failed to sync user settings: {e}");
            return Err((MailUserContextLoadingStage::User, e.into()));
        }

        trace!("Syncing event id");
        cb.on_stage(MailUserContextLoadingStage::Events);
        if let Err(e) = ctx.inner.event_loop.initialize(ctx, ctx).await {
            error!("Failed to sync event id:{e}");
            return Err((MailUserContextLoadingStage::Events, e.into()));
        }

        trace!("Syncing Addresses");
        cb.on_stage(MailUserContextLoadingStage::Addresses);
        if let Err(e) = ctx.sync_addresses().await {
            error!("Failed to sync addresses :{e}");
            return Err((MailUserContextLoadingStage::Addresses, e));
        }

        // load labels
        trace!("Syncing labels");
        cb.on_stage(MailUserContextLoadingStage::Labels);
        if let Err(e) = ctx.sync_labels().await {
            error!("Failed to sync labels: {e}");
            return Err((MailUserContextLoadingStage::Labels, e));
        }

        // load conversation counters
        trace!("Syncing conversation and message counts");
        cb.on_stage(MailUserContextLoadingStage::Counters);
        if let Err(e) = ctx.sync_conversation_and_message_counts().await {
            error!("Failed to sync conversation and messages counter: {e}");
            return Err((MailUserContextLoadingStage::Counters, e));
        }

        // load inbox conversations
        trace!("Syncing Inbox conversations");
        cb.on_stage(MailUserContextLoadingStage::Conversation);
        if let Err(e) = ctx.sync_first_conversation_page(label_id, 50).await {
            error!("Failed to sync Inbox conversation: {e}");
            return Err((MailUserContextLoadingStage::Conversation, e));
        }

        trace!("Syncing Complete");
        cb.on_stage(MailUserContextLoadingStage::Finished);
        Ok(())
    }
}
