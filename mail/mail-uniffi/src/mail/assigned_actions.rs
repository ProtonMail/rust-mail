use std::sync::Arc;

use crate::core::datatypes::Id;
use crate::errors::ActionError;
use crate::uniffi_async;
use uniffi::Enum as UniffiEnum;
use uniffi::Record as UniffiRecord;

use super::MailUserSession;
use super::datatypes::SystemLabel;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::datatypes::{
    AssignedSwipeAction as RealAssignedSwipeAction,
    AssignedSwipeActions as RealAssignedSwipeActions,
    SwipeActionMoveToTarget as RealSwipeActionMoveToTarget,
};

/// Contains information of what exactly has to happen when user swipes item (conversation, message)
/// right or left.
///
/// Note, this information is globally shared between all conversations and messages. User can set it in mail settings and it
/// does not depend on particular instance of message or conversation
///
#[derive(Clone, Debug, UniffiRecord)]
pub struct AssignedSwipeActions {
    pub left: AssignedSwipeAction,
    pub right: AssignedSwipeAction,
}

impl From<RealAssignedSwipeActions> for AssignedSwipeActions {
    fn from(value: RealAssignedSwipeActions) -> Self {
        Self {
            left: value.left.into(),
            right: value.right.into(),
        }
    }
}

/// Contains information of what exactly has to happen when user swipes item (conversation, message)
/// right or left.
///
/// This is different than [`SwipeAction`] as it contains extra information like label Remote ID.
///
#[derive(Clone, Debug, UniffiEnum)]
pub enum AssignedSwipeAction {
    NoAction,
    MoveTo(SwipeActionMoveToTarget),
    LabelAs,
    ToggleStar,
    ToggleRead,
}

impl From<RealAssignedSwipeAction> for AssignedSwipeAction {
    fn from(value: RealAssignedSwipeAction) -> Self {
        match value {
            RealAssignedSwipeAction::NoAction => Self::NoAction,
            RealAssignedSwipeAction::MoveTo(swipe_action_move_to_target) => {
                Self::MoveTo(swipe_action_move_to_target.into())
            }
            RealAssignedSwipeAction::LabelAs => Self::LabelAs,
            RealAssignedSwipeAction::ToggleStar => Self::ToggleStar,
            RealAssignedSwipeAction::ToggleRead => Self::ToggleRead,
        }
    }
}

#[derive(Clone, Debug, UniffiEnum)]
pub enum SwipeActionMoveToTarget {
    MoveToSystemLabel {
        label: SystemLabel,
        id: Id,
    },

    /// Swipe action requires extra popup for user to choose the target
    MoveToUnknownLabel,
}

impl From<RealSwipeActionMoveToTarget> for SwipeActionMoveToTarget {
    fn from(value: RealSwipeActionMoveToTarget) -> Self {
        match value {
            RealSwipeActionMoveToTarget::MoveToSystemLabel { label, id } => {
                Self::MoveToSystemLabel {
                    label: label.into(),
                    id: id.into(),
                }
            }
            RealSwipeActionMoveToTarget::MoveToUnknownLabel => Self::MoveToUnknownLabel,
        }
    }
}

#[uniffi_export]
pub async fn assigned_swipe_actions(
    current_folder: Id,
    session: Arc<MailUserSession>,
) -> Result<AssignedSwipeActions, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let actions = RealAssignedSwipeActions::get(current_folder.into(), &tether).await?;

        Ok::<_, RealProtonMailError>(AssignedSwipeActions::from(actions))
    })
    .await
    .map_err(ActionError::from)
}
