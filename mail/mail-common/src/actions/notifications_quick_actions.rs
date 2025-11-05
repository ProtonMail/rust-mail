use crate::datatypes::LocalMessageId;
use crate::datatypes::mail_notifications::InternalPushNotificationQuickAction;
use crate::models::Message;
use crate::{
    MailContextError, MailContextResult, MailUserContext,
    datatypes::mail_notifications::PushNotificationQuickAction,
};
use proton_action_queue::action::{
    Action, ActionId, Handler, Metadata, Priority, Type, VersionConverter, VersionConverterError,
    WriterGuard, deserialize,
};
use proton_core_api::exports::RetryPolicy;
use proton_core_api::session::Session;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{LabelError, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

use super::messages::{Move, MoveHandler, Read, ReadHandler};
use super::{ActionMoveData, MailActionError};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[instrument(skip(ctx))]
pub async fn exec(
    ctx: &MailUserContext,
    action: PushNotificationQuickAction,
    time_left_ms: Option<u64>,
) -> MailContextResult<()> {
    info!("Executing notification action");

    let time_left = time_left_ms.map(Duration::from_millis).map(|duration| {
        duration
            .saturating_sub(Duration::from_secs(1))
            .min(DEFAULT_TIMEOUT)
    });

    let action: InternalPushNotificationQuickAction = action.into();

    if let Err(err) = exec_flow(ctx, &action, time_left).await {
        warn!("Failed to execute {action:?}. Queuing fallback operation: {err}");

        ctx.user_context()
            .queue()
            .queue_action(PushNotificationAction {
                action,
                local_id: None,
            })
            .await?;
    }

    debug!("Finished executing notification action");

    Ok(())
}

#[instrument(skip(ctx))]
async fn exec_flow(
    ctx: &MailUserContext,
    action: &InternalPushNotificationQuickAction,
    time_left: Option<Duration>,
) -> MailContextResult<()> {
    let api = ctx.session();
    let retry_policy = Some(RetryPolicy::default().never());
    exec_remote(action, api, time_left, retry_policy).await?;
    exec_locally(ctx, action).await?;
    Ok(())
}

#[instrument(skip(ctx))]
async fn exec_locally(
    ctx: &MailUserContext,
    action: &InternalPushNotificationQuickAction,
) -> MailContextResult<()> {
    let tether = ctx.user_stash().connection().await?;
    let remote_id = action.remote_id();
    let local_id = Message::remote_id_counterpart(remote_id.clone(), &tether)
        .await?
        .ok_or_else(|| MailContextError::Other(anyhow::anyhow!("Message is not found")))?;

    let metadata = Metadata::builder()
        .with_priority_override(Priority::Highest)
        .build();

    match action {
        InternalPushNotificationQuickAction::MarkAsRead { remote_id: _ } => {
            ctx.action_queue()
                .queue_action_with_metadata(Read::for_push_notification(local_id), metadata)
                .await?;
        }
        InternalPushNotificationQuickAction::MoveToLabel {
            remote_id: _,
            label,
        } => {
            if let Some(action) = get_action_move_data(*label, local_id, &tether).await? {
                ctx.action_queue()
                    .queue_action_with_metadata(Move(action), metadata)
                    .await?;
            }
        }
    }

    Ok(())
}

#[instrument(skip(tether))]
async fn get_action_move_data(
    label: SystemLabel,
    local_id: LocalMessageId,
    tether: &Tether,
) -> Result<Option<ActionMoveData<Message>>, MailActionError> {
    // The likelihood of this failing is extremely low since system labels are
    // pre-created ahead of time.
    let label_id = label
        .local_id(tether)
        .await?
        .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(label.remote_id()))?;

    let Some(mut action_data) = ActionMoveData::new(tether, label_id, [local_id]).await? else {
        return Ok(None);
    };

    action_data.disable_remote();

    Ok(Some(action_data))
}

#[instrument(skip(session))]
async fn exec_remote(
    action: &InternalPushNotificationQuickAction,
    session: &Session,
    time_left: Option<Duration>,
    retry_policy: Option<RetryPolicy>,
) -> Result<(), MailActionError> {
    match &action {
        InternalPushNotificationQuickAction::MarkAsRead { remote_id } => {
            tracing::info!("Marking {remote_id:?} as read from push notification quick action");
            session
                .put_messages_read(vec![remote_id.clone()], time_left, retry_policy)
                .await?;
        }
        InternalPushNotificationQuickAction::MoveToLabel { remote_id, label } => {
            tracing::info!("Moving {remote_id:?} to {label:?} from push notification quick action");
            session
                .put_messages_label(
                    vec![remote_id.clone()],
                    label.label_id(),
                    None,
                    time_left,
                    retry_policy,
                )
                .await?;
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushNotificationAction {
    action: InternalPushNotificationQuickAction,
    local_id: Option<LocalMessageId>,
}

#[derive(Debug, Serialize, Deserialize)]
struct V0PushNotificationAction {
    action: PushNotificationQuickAction,
}

pub struct PushNotificationActionConverter;
impl VersionConverter for PushNotificationActionConverter {
    type Output = PushNotificationAction;

    fn convert(
        old_version: u32,
        current_version: u32,
        data: &[u8],
    ) -> proton_action_queue::action::FactoryResult<Self::Output> {
        if !(old_version <= 1 && current_version == 1) {
            return Err(VersionConverterError::InvalidVersion(current_version).into());
        }
        let v0 = deserialize::<V0PushNotificationAction>(data)?;
        let internal: InternalPushNotificationQuickAction = v0.action.into();

        Ok(PushNotificationAction {
            action: internal,
            local_id: None,
        })
    }
}

impl Action for PushNotificationAction {
    const TYPE: Type = Type("push_notification_quick_action");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Highest;

    type VersionConverter = PushNotificationActionConverter;
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
        action_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        // We still try to apply locally, because there might be a scenario,
        // where the message was synced in between of the push notification quick action execution
        // and queue processing.
        let remote_id = action.action.remote_id().clone();
        let Some(local_id) = Message::remote_id_counterpart(remote_id, tx).await? else {
            return Ok(());
        };
        action.local_id = Some(local_id);
        match &action.action {
            InternalPushNotificationQuickAction::MarkAsRead { remote_id: _ } => {
                ReadHandler {
                    api: self.api.clone(),
                }
                .apply_local(action_id, &mut Read::for_push_notification(local_id), tx)
                .await?;
            }
            InternalPushNotificationQuickAction::MoveToLabel {
                remote_id: _,
                label,
            } => {
                if let Some(action) = get_action_move_data(*label, local_id, tx).await? {
                    MoveHandler {
                        api: self.api.clone(),
                    }
                    .apply_local(action_id, &mut Move(action), tx)
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn revert_local(
        &self,
        action_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        let Some(local_id) = action.local_id else {
            return Ok(());
        };
        match &action.action {
            InternalPushNotificationQuickAction::MarkAsRead { remote_id: _ } => {
                ReadHandler {
                    api: self.api.clone(),
                }
                .revert_local(action_id, &mut Read::for_push_notification(local_id), tx)
                .await?;
            }
            InternalPushNotificationQuickAction::MoveToLabel {
                remote_id: _,
                label,
            } => {
                if let Some(action) = get_action_move_data(*label, local_id, tx).await? {
                    MoveHandler {
                        api: self.api.clone(),
                    }
                    .revert_local(action_id, &mut Move(action), tx)
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        exec_remote(&action.action, &self.api, None, None).await?;
        Ok(())
    }
}
