//! Generic mobile actions and shared logic for mobile toolbar actions
//!
//! This module provides a unified approach to mobile action handling that can be used
//! across different contexts (list, message, conversation) while sharing common logic
//! and maintaining type safety through generic implementations.

use crate::actions::MovableSystemFolderAction;
use crate::datatypes::SystemLabelId;
use crate::decrypted_message::ThemeOpts;
use proton_core_api::services::proton::LabelId;

/// Common actions shared between ListAction and MessageAction
#[derive(Debug, Clone, PartialEq)]
pub enum GenericAction {
    // Read state
    MarkRead,
    MarkUnread,

    // Star state
    Star,
    Unstar,

    // Organization
    LabelAs,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolderAction),
    NotSpam(MovableSystemFolderAction),
    PermanentDelete,

    // Utility
    More,
}

impl GenericAction {
    /// Toggle read state based on current state
    /// For single items: any_unread = item.is_unread
    /// For collections: any_unread = items.any(|item| item.is_unread)
    pub fn toggle_read(any_unread: bool) -> Self {
        if any_unread {
            Self::MarkRead
        } else {
            Self::MarkUnread
        }
    }

    /// Toggle star state based on current state
    /// For single items: any_starred = item.is_starred
    /// For collections: any_starred = items.any(|item| item.is_starred)
    pub fn toggle_star(any_starred: bool) -> Self {
        if any_starred {
            Self::Unstar
        } else {
            Self::Star
        }
    }

    /// Toggle star state with full context for collections
    /// Prioritizes Star action for mixed starred/unstarred collections
    pub fn toggle_star_with_context(any_starred: bool, all_starred: bool) -> Self {
        if any_starred && !all_starred {
            // Mixed case: some starred, some not - prioritize Star action
            // This allows starring the unstarred items (more common use case)
            Self::Star
        } else if all_starred {
            // All starred - only unstar available
            Self::Unstar
        } else {
            // None starred - only star available
            Self::Star
        }
    }

    /// Get archive action based on current label context
    pub fn toggle_archive(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
    ) -> Self {
        if current_label == &LabelId::archive() {
            Self::MoveToSystemFolder(*inbox)
        } else {
            Self::MoveToSystemFolder(*archive)
        }
    }

    /// Get trash action based on current label context
    pub fn toggle_trash(current_label: &LabelId, trash: &MovableSystemFolderAction) -> Self {
        if [LabelId::trash(), LabelId::spam()].contains(current_label) {
            Self::PermanentDelete
        } else {
            Self::MoveToSystemFolder(*trash)
        }
    }

    /// Get spam action based on current label context
    pub fn toggle_spam(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Self {
        if current_label == &LabelId::spam() {
            Self::NotSpam(*inbox)
        } else if current_label == &LabelId::trash() {
            Self::MoveToSystemFolder(*inbox)
        } else {
            Self::MoveToSystemFolder(*spam)
        }
    }
}

/// Context information needed for building actions
#[derive(Debug, Clone)]
pub struct ActionContext {
    pub current_label: LabelId,
    pub any_unread: bool, // For single items: item.is_unread, for collections: items.any(|i| i.is_unread)
    pub any_read: bool, // For single items: !item.is_unread, for collections: items.any(|i| !i.is_unread)
    pub all_read: bool, // For single items: !item.is_unread, for collections: items.all(|i| !i.is_unread)
    pub any_starred: bool, // For single items: item.is_starred, for collections: items.any(|i| i.is_starred)
    pub all_starred: bool, // For single items: item.is_starred, for collections: items.all(|i| i.is_starred)
    pub theme: Option<ThemeOpts>, // Optional - only needed for message actions with theme-specific views
    pub folders: SystemFolders,
    // Message-specific context
    pub can_reply: bool,
    pub can_reply_all: bool,
    // List-specific context
    pub is_conversation: bool,
}

/// System folders used in actions
#[derive(Debug, Clone)]
pub struct SystemFolders {
    pub inbox: MovableSystemFolderAction,
    pub archive: MovableSystemFolderAction,
    pub trash: MovableSystemFolderAction,
    pub spam: MovableSystemFolderAction,
}

/// Common behavior shared between ListAction and MessageAction
pub trait GenericMobileActions: Clone + PartialEq + Sized + From<GenericAction> {
    /// Convert MobileAction to specific action type with context
    fn from_mobile_action(
        mobile_action: &crate::datatypes::MobileAction,
        context: &ActionContext,
    ) -> Option<Self>;

    /// Check if two actions are counter-actions (e.g., MarkRead vs MarkUnread)
    fn are_counter_actions(action1: &Self, action2: &Self) -> bool;

    fn toggle_read(any_unread: bool) -> Self {
        GenericAction::toggle_read(any_unread).into()
    }

    fn toggle_star(any_starred: bool) -> Self {
        GenericAction::toggle_star(any_starred).into()
    }

    fn toggle_star_with_context(any_starred: bool, all_starred: bool) -> Self {
        GenericAction::toggle_star_with_context(any_starred, all_starred).into()
    }

    fn toggle_archive(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
    ) -> Self {
        GenericAction::toggle_archive(current_label, inbox, archive).into()
    }

    fn toggle_trash(current_label: &LabelId, trash: &MovableSystemFolderAction) -> Self {
        GenericAction::toggle_trash(current_label, trash).into()
    }

    fn toggle_spam(
        current_label: &LabelId,
        inbox: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Self {
        GenericAction::toggle_spam(current_label, inbox, spam).into()
    }

    fn mark_read() -> Self {
        GenericAction::MarkRead.into()
    }

    fn mark_unread() -> Self {
        GenericAction::MarkUnread.into()
    }

    fn star() -> Self {
        GenericAction::Star.into()
    }

    fn unstar() -> Self {
        GenericAction::Unstar.into()
    }

    fn label_as() -> Self {
        GenericAction::LabelAs.into()
    }

    fn move_to() -> Self {
        GenericAction::MoveTo.into()
    }

    fn move_to_system_folder(folder: MovableSystemFolderAction) -> Self {
        GenericAction::MoveToSystemFolder(folder).into()
    }

    fn not_spam(folder: MovableSystemFolderAction) -> Self {
        GenericAction::NotSpam(folder).into()
    }

    fn permanent_delete() -> Self {
        GenericAction::PermanentDelete.into()
    }

    fn more() -> Self {
        GenericAction::More.into()
    }

    fn get_high_priority_actions(_context: &ActionContext) -> Vec<Self> {
        vec![]
    }

    fn get_low_priority_actions(_context: &ActionContext) -> Vec<Self> {
        vec![]
    }
}
