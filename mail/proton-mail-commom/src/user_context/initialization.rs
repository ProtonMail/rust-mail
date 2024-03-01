use crate::{MailContext, MailContextError, MailUserContext};
use proton_api_mail::domain::LabelId;
use proton_api_mail::proton_api_core::exports::tracing::{self, Level};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    User,
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
    #[tracing::instrument(level = Level::DEBUG, skip(self, mail_context,cb), fields(user_id=?self.0.user_id()))]
    pub fn initialize(
        &self,
        mail_context: &MailContext,
        label_id: LabelId,
        cb: Box<dyn MailUserContextInitializationCallback>,
    ) {
        let ctx = self.clone();
        mail_context.async_runtime().spawn(async move {
            // initialize user context?
            cb.on_stage(MailUserContextLoadingStage::User);

            // load labels
            cb.on_stage(MailUserContextLoadingStage::Labels);
            if let Err(e) = ctx.sync_labels().await {
                cb.on_stage_err(MailUserContextLoadingStage::Labels, e);
                return;
            }

            // load conversation counters
            cb.on_stage(MailUserContextLoadingStage::Counters);

            // load inbox conversations
            cb.on_stage(MailUserContextLoadingStage::Conversation);
            if let Err(e) = ctx.sync_first_conversation_page(label_id, 50).await {
                cb.on_stage_err(MailUserContextLoadingStage::Conversation, e);
                return;
            }
            cb.on_stage(MailUserContextLoadingStage::Finished);
        });
    }
}
