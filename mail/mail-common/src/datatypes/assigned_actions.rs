#[cfg(test)]
#[path = "../tests/datatypes/assigned_actions.rs"]
mod tests;

use super::SwipeAction;
use crate::{AppError, models::MailSettings};
use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use stash::stash::Tether;

#[derive(Clone, Debug)]
pub struct AssignedSwipeActions {
    pub left: AssignedSwipeAction,
    pub right: AssignedSwipeAction,
}

impl AssignedSwipeActions {
    pub async fn get(current_folder: LocalLabelId, tether: &Tether) -> Result<Self, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        Ok(Self {
            left: AssignedSwipeAction::load(settings.swipe_left, current_folder, tether).await?,
            right: AssignedSwipeAction::load(settings.swipe_right, current_folder, tether).await?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignedSwipeAction {
    NoAction,
    MoveTo(SwipeActionMoveToTarget),
    LabelAs,
    ToggleStar,
    ToggleRead,
}

impl AssignedSwipeAction {
    pub async fn load(
        swipe_action: SwipeAction,
        current_folder: LocalLabelId,
        tether: &Tether,
    ) -> Result<Self, AppError> {
        let move_to = match swipe_action {
            SwipeAction::NoAction => return Ok(Self::NoAction),
            SwipeAction::Star => return Ok(Self::ToggleStar),
            SwipeAction::MarkAsRead => return Ok(Self::ToggleRead),
            SwipeAction::LabelAs => return Ok(Self::LabelAs),
            SwipeAction::MoveTo => {
                return Ok(Self::MoveTo(SwipeActionMoveToTarget::MoveToUnknownLabel));
            }
            // These actions are just specific hardcoded variants of MoveTo action
            SwipeAction::Trash => SystemLabel::Trash,
            SwipeAction::Spam => SystemLabel::Spam,
            SwipeAction::Archive => SystemLabel::Archive,
        };

        let label_id = move_to
            .local_id(tether)
            .await?
            .ok_or_else(|| AppError::RemoteLabelDoesNotExist(move_to.remote_id()))?;

        if label_id == current_folder {
            return Ok(Self::NoAction);
        }

        Ok(Self::MoveTo(SwipeActionMoveToTarget::MoveToSystemLabel {
            label: move_to,
            id: label_id,
        }))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SwipeActionMoveToTarget {
    MoveToSystemLabel {
        label: SystemLabel,
        id: LocalLabelId,
    },

    /// Swipe action requires extra popup for user to choose the target
    MoveToUnknownLabel,
}
