use crate::{
    MailContextError, MailContextResult, MailUserContext,
    actions::{
        ActionMoveData,
        messages::{Move, Read},
    },
    datatypes::mail_notifications::PushNotificationQuickAction,
    models::Message,
};
use proton_action_queue::action::{Metadata, Priority};
use proton_core_common::{
    actions::event_poll::EventPoll, datatypes::SystemLabel, models::LabelError,
};
use proton_mail_api::services::proton::common::MessageId;
use std::iter;
use tracing::{instrument, warn};

#[instrument(skip(ctx))]
pub async fn execute_notification_quick_action(
    ctx: &MailUserContext,
    action: PushNotificationQuickAction,
) -> MailContextResult<()> {
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
    let msg_id = Message::find_or_fetch_by_remote_id(ctx, msg_id).await?;

    ctx.queue_action(Read::new(iter::once(msg_id))).await?;

    Ok(())
}

#[instrument(skip_all)]
async fn move_msg(
    ctx: &MailUserContext,
    label: SystemLabel,
    msg_id: MessageId,
) -> MailContextResult<()> {
    let msg_id = Message::find_or_fetch_by_remote_id(ctx, msg_id).await?;
    let tether = ctx.user_stash().connection().await?;

    let label_id = label
        .local_id(&tether)
        .await?
        .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(label.remote_id()))?;

    if let Some(action) = ActionMoveData::new(&tether, label_id, [msg_id]).await? {
        let action = ctx.queue_action(Move(action)).await?;

        ctx.user_context()
            .queue()
            .queue_action_with_metadata(
                EventPoll {},
                Metadata::builder()
                    .with_dependency(action.id)
                    .with_priority_override(Priority::High)
                    .build(),
            )
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't poll event loop");
                MailContextError::Other(err.into())
            })?;
    }

    Ok(())
}
