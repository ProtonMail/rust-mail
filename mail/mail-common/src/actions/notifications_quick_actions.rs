use crate::datatypes::SystemLabelId;
use crate::models::Message;
use crate::{
    MailContextError, MailContextResult, MailUserContext,
    datatypes::mail_notifications::PushNotificationQuickAction,
};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::LabelError;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{info, instrument, warn};

use super::ActionMoveData;
use super::messages::{Move, Read};

#[instrument(skip(ctx))]
pub async fn exec(
    ctx: &MailUserContext,
    action: PushNotificationQuickAction,
) -> MailContextResult<()> {
    info!("Executing notification action");

    match action {
        PushNotificationQuickAction::MarkAsRead { remote_id } => {
            read_msg(ctx, remote_id).await?;
        }
        PushNotificationQuickAction::MoveToArchive { remote_id } => {
            move_msg(ctx, SystemLabel::Archive, remote_id).await?;
        }
        PushNotificationQuickAction::MoveToTrash { remote_id } => {
            move_msg(ctx, SystemLabel::Trash, remote_id).await?;
        }
    }

    Ok(())
}

#[instrument(skip_all)]
async fn read_msg(ctx: &MailUserContext, msg_id: MessageId) -> MailContextResult<()> {
    match Message::find_or_fetch_by_remote_id(ctx, msg_id.clone()).await {
        Ok(local_id) => {
            ctx.queue_action(Read::new(std::iter::once(local_id)))
                .await?;
        }
        Err(e) => {
            warn!("Failed to resolve remote id, queuing fallback operation: {e}");
            ctx.action_queue()
                .queue_action(PushNotificationAction {
                    action: PushNotificationQuickAction::MarkAsRead { remote_id: msg_id },
                })
                .await?;
        }
    }
    Ok(())
}

#[instrument(skip_all)]
async fn move_msg(
    ctx: &MailUserContext,
    label: SystemLabel,
    msg_id: MessageId,
) -> MailContextResult<()> {
    match Message::find_or_fetch_by_remote_id(ctx, msg_id.clone()).await {
        Ok(local_id) => {
            let tether = ctx.user_stash().connection().await?;

            // The likelihood of this failing is extremely low since system labels are
            // pre-created ahead of time.
            let label_id = label
                .local_id(&tether)
                .await?
                .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(label.remote_id()))?;

            if let Some(action) = ActionMoveData::new(&tether, label_id, [local_id]).await? {
                ctx.queue_action(Move(action)).await?;
            }
        }
        Err(e) => {
            warn!("Failed to resolve remote id, queuing fallback operation: {e}");
            if label == SystemLabel::Archive {
                ctx.action_queue()
                    .queue_action(PushNotificationAction {
                        action: PushNotificationQuickAction::MoveToArchive { remote_id: msg_id },
                    })
                    .await?;
            } else if label == SystemLabel::Trash {
                ctx.action_queue()
                    .queue_action(PushNotificationAction {
                        action: PushNotificationQuickAction::MoveToTrash { remote_id: msg_id },
                    })
                    .await?;
            } else {
                warn!("Received invalid system label: {label}");
                return Ok(());
            }
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
        match &action.action {
            PushNotificationQuickAction::MarkAsRead { remote_id } => {
                tracing::info!("Marking {remote_id:?} as read from push notification quick action");
                self.api.put_messages_read(vec![remote_id.clone()]).await?;
            }
            PushNotificationQuickAction::MoveToArchive { remote_id } => {
                tracing::info!(
                    "Moving {remote_id:?} to Archive from push notification quick action"
                );
                self.api
                    .put_messages_label(vec![remote_id.clone()], LabelId::archive(), None)
                    .await?;
            }
            PushNotificationQuickAction::MoveToTrash { remote_id } => {
                tracing::info!("Moving {remote_id:?} to Trash from push notification quick action");
                self.api
                    .put_messages_label(vec![remote_id.clone()], LabelId::trash(), None)
                    .await?;
            }
        }
        Ok(())
    }
}
