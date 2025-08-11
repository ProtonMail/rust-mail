use crate::{
    MailContextResult, MailUserContext,
    actions::{
        ActionMoveData,
        messages::{Move, Read},
    },
    datatypes::mail_notifications::PushNotificationQuickAction,
    models::Message,
};
use proton_core_common::{datatypes::SystemLabel, models::LabelError};
use proton_mail_api::services::proton::common::MessageId;
use std::iter;
use tracing::instrument;

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
    let tether = ctx.user_stash().connection();

    let label_id = label
        .local_id(&tether)
        .await?
        .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(label.remote_id()))?;

    if let Some(action) = ActionMoveData::new(&tether, label_id, [msg_id]).await? {
        ctx.queue_action(Move(action)).await?;
    }

    Ok(())
}
