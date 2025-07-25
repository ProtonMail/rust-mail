use proton_core_common::{datatypes::SystemLabel, models::LabelError};
use proton_mail_api::services::proton::common::MessageId;

use crate::{
    MailContextResult, MailUserContext,
    actions::{
        ActionMoveData,
        messages::{Move, Read},
    },
    datatypes::mail_notifications::PushNotificationQuickAction,
    models::Message,
};

/// Insert the quick action into the queue and execute local part immediately.
///
pub async fn execute_notification_quick_action(
    ctx: &MailUserContext,
    action: PushNotificationQuickAction,
) -> MailContextResult<()> {
    match action {
        PushNotificationQuickAction::MarkAsRead { remote_id } => {
            let local_id = Message::find_or_fetch_by_remote_id(ctx, remote_id).await?;

            ctx.queue_action(Read::new(std::iter::once(local_id)))
                .await?;
        }
        PushNotificationQuickAction::MoveToArchive { remote_id } => {
            move_to_system_label(ctx, SystemLabel::Archive, remote_id).await?;
        }
        PushNotificationQuickAction::MoveToTrash { remote_id } => {
            move_to_system_label(ctx, SystemLabel::Trash, remote_id).await?;
        }
    }
    Ok(())
}

async fn move_to_system_label(
    ctx: &MailUserContext,
    system_label: SystemLabel,
    remote_id: MessageId,
) -> MailContextResult<()> {
    let local_id = Message::find_or_fetch_by_remote_id(ctx, remote_id).await?;
    let tether = ctx.user_stash().connection();

    let destination_label = system_label
        .local_id(&tether)
        .await?
        .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(system_label.remote_id()))?;

    if let Some(move_action) = ActionMoveData::new(&tether, destination_label, [local_id]).await? {
        ctx.queue_action(Move(move_action)).await?;
    }
    Ok(())
}
