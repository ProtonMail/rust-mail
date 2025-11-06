use crate::datatypes::LocalMessageId;
use crate::models::Message;
use crate::{
    MailContextResult, MailUserContext, datatypes::mail_notifications::PushNotificationQuickAction,
};
use proton_action_queue::action::{
    Action, ActionId, Handler, Priority, Type, VersionConverter, VersionConverterError,
    WriterGuard, deserialize,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::exports::RetryPolicy;
use proton_core_api::session::Session;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{LabelError, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

use super::messages::{Move, MoveHandler, Read, ReadHandler};
use super::{ActionMoveData, MailActionError};

// This timeout is explicitly set to 30 not 60 seconds because there is no chance
// we will get more than 30 seconds for the notification execution from the OS.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

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

    let state: PushNotificationActionState = action.into();

    let apply_remotely = exec_remote(
        &state,
        ctx.session(),
        time_left,
        Some(RetryPolicy::default().never()),
    )
    .await
    .inspect_err(|err| warn!("Failed to execute {state:?} remotely: {err}"))
    .is_err();

    ctx.user_context()
        .queue()
        .queue_action(PushNotificationAction {
            state,
            apply_remotely,
        })
        .await?;

    debug!("Finished executing notification action");

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

    let data = ActionMoveData::new(tether, label_id, [local_id]).await?;

    Ok(data)
}

#[instrument(skip(session))]
async fn exec_remote(
    state: &PushNotificationActionState,
    session: &Session,
    time_left: Option<Duration>,
    retry_policy: Option<RetryPolicy>,
) -> Result<(), MailActionError> {
    match &state {
        PushNotificationActionState::MarkAsRead {
            remote_id,
            local_action: _,
        } => {
            tracing::info!("Marking {remote_id:?} as read from push notification quick action");
            session
                .put_messages_read_ex(vec![remote_id.clone()], time_left, retry_policy)
                .await?;
        }
        PushNotificationActionState::MoveToLabel {
            remote_id,
            label,
            local_action: _,
        } => {
            tracing::info!("Moving {remote_id:?} to {label:?} from push notification quick action");
            session
                .put_messages_label_ex(
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PushNotificationActionState {
    MarkAsRead {
        remote_id: MessageId,
        local_action: Option<Read>,
    },
    MoveToLabel {
        remote_id: MessageId,
        label: SystemLabel,
        local_action: Option<Move>,
    },
}

impl From<PushNotificationQuickAction> for PushNotificationActionState {
    fn from(action: PushNotificationQuickAction) -> Self {
        match action {
            PushNotificationQuickAction::MarkAsRead { remote_id } => Self::MarkAsRead {
                remote_id,
                local_action: None,
            },
            PushNotificationQuickAction::MoveToArchive { remote_id } => Self::MoveToLabel {
                remote_id,
                label: SystemLabel::Archive,
                local_action: None,
            },
            PushNotificationQuickAction::MoveToTrash { remote_id } => Self::MoveToLabel {
                remote_id,
                label: SystemLabel::Trash,
                local_action: None,
            },
        }
    }
}

impl PushNotificationActionState {
    pub fn remote_id(&self) -> &MessageId {
        match self {
            Self::MarkAsRead { remote_id, .. } => remote_id,
            Self::MoveToLabel { remote_id, .. } => remote_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushNotificationAction {
    state: PushNotificationActionState,
    apply_remotely: bool,
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
        if old_version == current_version {
            return Ok(deserialize::<PushNotificationAction>(data)?);
        }
        let v0 = deserialize::<V0PushNotificationAction>(data)?;
        let state: PushNotificationActionState = v0.action.into();

        Ok(PushNotificationAction {
            state,
            apply_remotely: true,
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

impl PushNotificationActionHandler {
    fn read_handler(&self) -> ReadHandler {
        ReadHandler {
            api: self.api.clone(),
        }
    }

    fn move_handler(&self) -> MoveHandler {
        MoveHandler {
            api: self.api.clone(),
        }
    }
}

impl Handler for PushNotificationActionHandler {
    type Action = PushNotificationAction;

    async fn apply_local(
        &self,
        action_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        let remote_id = action.state.remote_id().clone();
        // There might be a rare case where the push notification arrived after the message was already synced via the event poll.
        // In most cases, this `remote_id_counterpart` will return None tho.
        let Some(local_id) = Message::remote_id_counterpart(remote_id, tx).await? else {
            return Ok(());
        };
        match &mut action.state {
            PushNotificationActionState::MarkAsRead {
                remote_id: _,
                local_action,
            } => {
                let mut action = Read::single(local_id);
                self.read_handler()
                    .apply_local(action_id, &mut action, tx)
                    .await?;
                *local_action = Some(action);
            }
            PushNotificationActionState::MoveToLabel {
                remote_id: _,
                label,
                local_action,
            } => {
                if let Some(action_data) = get_action_move_data(*label, local_id, tx).await? {
                    let mut action = Move(action_data);
                    self.move_handler()
                        .apply_local(action_id, &mut action, tx)
                        .await?;
                    *local_action = Some(action);
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
        match &mut action.state {
            PushNotificationActionState::MarkAsRead {
                remote_id: _,
                local_action: Some(local_action),
            } => {
                self.read_handler()
                    .revert_local(action_id, local_action, tx)
                    .await?;
            }
            PushNotificationActionState::MoveToLabel {
                remote_id: _,
                label: _,
                local_action: Some(local_action),
            } => {
                self.move_handler()
                    .revert_local(action_id, local_action, tx)
                    .await?;
            }
            _ => (),
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        if action.apply_remotely {
            exec_remote(&action.state, &self.api, None, None).await?;
        }
        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
