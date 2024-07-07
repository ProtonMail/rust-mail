use crate::models::{Conversation, Label, MailSettings};
use crate::{MailContextError, MailUserContext};
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LabelId;
use proton_core_common::models::{Address, User};
use tracing::{error, trace, Level};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    User,
    MailSettings,
    Addresses,
    Events,
    Labels,
    Counters,
    Finished,
}
pub trait MailUserContextInitializationCallback: Send + Sync + 'static {
    fn on_stage(&self, stage: MailUserContextLoadingStage);
    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError);
}

impl MailUserContext {
    #[tracing::instrument(level = Level::DEBUG, skip(self, cb), fields(user_id=?self.user_id()))]
    pub async fn initialize(
        &self,
        label_id: LabelId,
        cb: Box<dyn MailUserContextInitializationCallback>,
    ) {
        if let Err((stage, err)) = self.initialize_async(label_id, cb.as_ref()).await {
            cb.on_stage_err(stage, err);
        }
    }

    #[tracing::instrument(level = Level::DEBUG, skip(self, cb), fields(user_id=?self.user_id()))]
    pub async fn initialize_async(
        &self,
        label_id: LabelId,
        cb: &dyn MailUserContextInitializationCallback,
    ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
        let ctx = self;

        trace!("Syncing event id");
        cb.on_stage(MailUserContextLoadingStage::Events);
        if let Err(e) = ctx.event_loop.initialize(ctx, ctx).await {
            error!("Failed to sync event id:{e}");
            return Err((MailUserContextLoadingStage::Events, e.into()));
        }

        trace!("Syncing User settings");
        cb.on_stage(MailUserContextLoadingStage::User);
        if let Err(e) = User::sync_user_and_settings(ctx.session().api(), ctx.stash()).await {
            error!("Failed to sync user settings: {e}");
            return Err((MailUserContextLoadingStage::User, e.into()));
        }

        trace!("Syncing Mail settings");
        cb.on_stage(MailUserContextLoadingStage::MailSettings);
        if let Err(e) = MailSettings::sync_mail_settings(ctx.session().api(), ctx.stash()).await {
            error!("Failed to sync user settings: {e}");
            return Err((MailUserContextLoadingStage::MailSettings, e.into()));
        }

        trace!("Syncing Addresses");
        cb.on_stage(MailUserContextLoadingStage::Addresses);
        if let Err(e) = Address::sync(ctx.session().api(), ctx.stash()).await {
            error!("Failed to sync addresses :{e}");
            return Err((MailUserContextLoadingStage::Addresses, e.into()));
        }

        // load labels
        trace!("Syncing labels");
        cb.on_stage(MailUserContextLoadingStage::Labels);
        if let Err(e) = Label::sync_labels(ctx.session().api(), ctx.stash()).await {
            error!("Failed to sync labels: {e}");
            return Err((MailUserContextLoadingStage::Labels, e.into()));
        }

        // load conversation counters
        trace!("Syncing conversation and message counts");
        cb.on_stage(MailUserContextLoadingStage::Counters);
        if let Err(e) =
            Conversation::sync_conversation_and_message_counts(ctx.session().api(), ctx.stash())
                .await
        {
            error!("Failed to sync conversation and messages counter: {e}");
            return Err((MailUserContextLoadingStage::Counters, e.into()));
        }

        trace!("Syncing Complete");
        cb.on_stage(MailUserContextLoadingStage::Finished);
        Ok(())
    }
}
