use crate::datatypes::SystemLabelId;
use crate::models::Message;
use crate::{
    MailContextError, MailContextResult, MailUserContext,
    datatypes::mail_notifications::PushNotificationQuickAction,
};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Metadata, Priority, Type, WriterGuard,
};
use proton_action_queue::queue::QueuedActionOutput;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{LabelError, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{info, instrument, warn};

use super::messages::{Move, Read};
use super::{ActionMoveData, MailActionError};

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

async fn read_msg_local(ctx: &MailUserContext, msg_id: MessageId) -> MailContextResult<()> {
    let tether = ctx.user_stash().connection().await?;
    let local_id = Message::remote_id_counterpart(msg_id.clone(), &tether)
        .await?
        .ok_or_else(|| MailContextError::Other(anyhow::anyhow!("Message is not found")))?;
    queue_action_with_highest_priority(ctx, Read::new(std::iter::once(local_id))).await?;
    Ok(())
}

#[instrument(skip_all)]
async fn read_msg(ctx: &MailUserContext, msg_id: MessageId) -> MailContextResult<()> {
    if let Err(e) = read_msg_local(ctx, msg_id.clone()).await {
        warn!("Failed to mark as read locally. Queuing fallback operation: {e}");

        queue_action_with_highest_priority(
            ctx,
            PushNotificationAction {
                action: PushNotificationQuickAction::MarkAsRead { remote_id: msg_id },
            },
        )
        .await?;
    }
    Ok(())
}

async fn move_msg_local(
    ctx: &MailUserContext,
    msg_id: MessageId,
    label: SystemLabel,
) -> MailContextResult<()> {
    let tether = ctx.user_stash().connection().await?;

    let local_id = Message::remote_id_counterpart(msg_id.clone(), &tether)
        .await?
        .ok_or_else(|| MailContextError::Other(anyhow::anyhow!("Message is not found")))?;

    // The likelihood of this failing is extremely low since system labels are
    // pre-created ahead of time.
    let label_id = label
        .local_id(&tether)
        .await?
        .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(label.remote_id()))?;

    if let Some(action) = ActionMoveData::new(&tether, label_id, [local_id]).await? {
        queue_action_with_highest_priority(ctx, Move(action)).await?;
    }

    Ok(())
}

#[instrument(skip_all)]
async fn move_msg(
    ctx: &MailUserContext,
    label: SystemLabel,
    msg_id: MessageId,
) -> MailContextResult<()> {
    if let Err(e) = move_msg_local(ctx, msg_id.clone(), label).await {
        warn!("Failed to move message locally. Queuing fallback operation: {e}");
        if label == SystemLabel::Archive {
            queue_action_with_highest_priority(
                ctx,
                PushNotificationAction {
                    action: PushNotificationQuickAction::MoveToArchive { remote_id: msg_id },
                },
            )
            .await?;
        } else if label == SystemLabel::Trash {
            queue_action_with_highest_priority(
                ctx,
                PushNotificationAction {
                    action: PushNotificationQuickAction::MoveToTrash { remote_id: msg_id },
                },
            )
            .await?;
        } else {
            warn!("Received invalid system label: {label}");
            return Ok(());
        }
    }
    Ok(())
}

async fn queue_action_with_highest_priority<T>(
    ctx: &MailUserContext,
    action: T,
) -> MailContextResult<QueuedActionOutput<T>>
where
    T: Action<Error = MailActionError>,
{
    Ok(ctx
        .user_context()
        .queue()
        .queue_action_with_metadata(
            action,
            Metadata::builder()
                .with_priority_override(Priority::Highest)
                .build(),
        )
        .await?)
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
    type Error = MailActionError;
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
    ) -> Result<(), MailActionError> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
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
