use std::sync::Arc;

use crate::core::datatypes::Id;
use crate::errors::ActionError;
use crate::uniffi_async;
use uniffi::Enum as UniffiEnum;
use uniffi::Record as UniffiRecord;

use super::MailUserSession;
use super::datatypes::SystemLabel;
use proton_mail_common::datatypes::{
    AssignedSwipeAction as RealAssignedSwipeAction,
    AssignedSwipeActions as RealAssignedSwipeActions,
    SwipeActionMoveToTarget as RealSwipeActionMoveToTarget,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

/// Contains information of what exactly has to happen when user swipes item (conversation, message)
/// right or left.
///
/// Note, this information is globally shared between all conversations and messages. User can set it in mail settings and it
/// does not depend on particular instance of message or conversation
///
#[derive(Clone, Debug, UniffiRecord)]
pub struct AssignedSwipeActions {
    /// When user swipes left
    pub left: AssignedSwipeAction,
    /// When user swipes right
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
    /// Swipe gesture is no-op
    NoAction,

    /// Swipe gesture moves item to another folder
    MoveTo(SwipeActionMoveToTarget),

    /// Swipe gesture labels item - it requires to open an extra popup for user to choose labels
    LabelAs,

    /// Swipe gesture toggles star
    ToggleStar,

    /// Swipe gesture marks item as (un)read
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

/// When moving item to another folder, mobile app needs to either know where to move or that it has to open a new popup
///
#[derive(Clone, Debug, UniffiEnum)]
pub enum SwipeActionMoveToTarget {
    /// Swipe action is programmed to move to one of the special folders
    /// For example Trash, Archive, Spam etc.
    MoveToSystemLabel {
        /// To show the right icon
        label: SystemLabel,
        /// To pass as a parameter for `move_to` functions.
        /// Local ID
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

/// Returns assigned swipe actions based on user's mail settings.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
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
