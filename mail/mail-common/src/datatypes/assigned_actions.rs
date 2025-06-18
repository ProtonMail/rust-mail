#[cfg(test)]
#[path = "../tests/datatypes/assigned_actions.rs"]
mod tests;

use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use stash::stash::Tether;

use crate::{AppError, models::MailSettings};

use super::SwipeAction;

/// Contains information of what exactly has to happen when user swipes item (conversation, message)
/// right or left.
///
/// Note, this information is globally shared between all conversations and messages. User can set it in mail settings and it
/// does not depend on particular instance of message or conversation
///
#[derive(Clone, Debug)]
pub struct AssignedSwipeActions {
    /// When user swipes left
    pub left: AssignedSwipeAction,
    /// When user swipes right
    pub right: AssignedSwipeAction,
}

impl AssignedSwipeActions {
    /// Get assigned swipe actions by fetching user settings
    ///
    /// # Errors
    ///
    /// Returns an error if query fails
    ///
    pub async fn get(current_folder: LocalLabelId, tether: &Tether) -> Result<Self, AppError> {
        let settings = MailSettings::get_or_default(tether).await;

        Ok(Self {
            left: AssignedSwipeAction::load(settings.swipe_left, current_folder, tether).await?,
            right: AssignedSwipeAction::load(settings.swipe_right, current_folder, tether).await?,
        })
    }
}

/// Contains information of what exactly has to happen when user swipes item (conversation, message)
/// right or left.
///
/// This is different than [`SwipeAction`] as it contains extra information like label Remote ID.
///
#[derive(Clone, Debug, PartialEq)]
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

impl AssignedSwipeAction {
    /// Loads assigned swipe action based on the swipe action stored in the mail settings.
    ///
    /// # Errors
    ///
    /// Returns an error if query fails
    ///
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

/// When moving item to another folder, mobile app needs to either know where to move or that it has to open a new popup
///
#[derive(Clone, Debug, PartialEq)]
pub enum SwipeActionMoveToTarget {
    /// Swipe action is programmed to move to one of the special folders
    /// For example Trash, Archive, Spam etc.
    MoveToSystemLabel {
        /// To show the right icon
        label: SystemLabel,
        /// To pass as a parameter for `move_to` functions.
        id: LocalLabelId,
    },
    /// Swipe action requires extra popup for user to choose the target
    MoveToUnknownLabel,
}
