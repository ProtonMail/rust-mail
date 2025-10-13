use crate::datatypes::SystemLabelId;
use crate::{
    MailContextError, MailContextResult, MailUserContext,
    datatypes::mail_notifications::PushNotificationQuickAction,
};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{info, instrument, warn};

#[instrument(skip(ctx))]
pub async fn exec(
    ctx: &MailUserContext,
    action: PushNotificationQuickAction,
) -> MailContextResult<()> {
    info!("Executing notification action");

    if let Err(err) = act(ctx.session(), &action).await {
        warn!(
            ?err,
            "Couldn't act on the message, queueing action for later"
        );

        ctx.action_queue()
            .queue_action(PushNotificationAction { action })
            .await?;
    }

    Ok(())
}

#[instrument(skip_all)]
async fn act(api: &Session, action: &PushNotificationQuickAction) -> MailContextResult<()> {
    info!("Acting on message");

    match action {
        PushNotificationQuickAction::MarkAsRead { remote_id } => {
            api.put_messages_read(vec![remote_id.clone()]).await?;
        }

        PushNotificationQuickAction::MoveToArchive { remote_id } => {
            api.put_messages_label(vec![remote_id.clone()], LabelId::archive(), None)
                .await?;
        }

        PushNotificationQuickAction::MoveToTrash { remote_id } => {
            api.put_messages_label(vec![remote_id.clone()], LabelId::trash(), None)
                .await?;
        }
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushNotificationAction {
    action: PushNotificationQuickAction,
}

impl Action for PushNotificationAction {
    const TYPE: Type = Type("push_notification_quick_action");
    const VERSION: u32 = 0;
    const PRIORITY: Priority = Priority::Highest;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = PushNotificationActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct PushNotificationActionHandler {
    pub api: Session,
}

impl Handler for PushNotificationActionHandler {
    type Action = PushNotificationAction;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), MailContextError> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), MailContextError> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<(), MailContextError> {
        act(&self.api, &action.action).await
    }
}
