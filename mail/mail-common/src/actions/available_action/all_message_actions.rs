#[cfg(test)]
#[path = "../../tests/actions/available_actions/all_message_actions.rs"]
mod tests;

use crate::actions::MovableSystemFolderAction;
use crate::actions::{ActionContext, GenericAction, GenericMobileActions};
use crate::datatypes::{MobileAction, theme::MailTheme};
use crate::decrypted_message::ThemeOpts;
use proton_core_api::services::proton::LabelId;

/// Struct to reflect what kind of actions
/// could be taken upon the message.
///
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MessageActionSheet {
    pub reply_actions: Vec<MessageAction>,
    pub message_actions: Vec<MessageAction>,
    pub move_actions: Vec<MessageAction>,
    pub general_actions: Vec<MessageAction>,
}

impl From<AllMessageActions> for MessageActionSheet {
    fn from(value: AllMessageActions) -> Self {
        let mut this = Self::default();
        let all_actions = [value.visible_message_actions, value.hidden_message_actions].concat();

        all_actions.iter().for_each(|action| {
            if action.is_reply_action() {
                this.reply_actions.push(*action);
            } else if action.is_move_action() {
                this.move_actions.push(*action);
            } else if action.is_general_action() {
                this.general_actions.push(*action);
            } else if action.is_message_action() {
                this.message_actions.push(*action);
            }
        });

        this
    }
}

/// All actions on message selection for Phase 2 dynamic actions.
#[derive(Debug, Clone, PartialEq)]
pub struct AllMessageActions {
    /// Actions hidden in message toolbar, but to be shown in corresponding More action
    pub hidden_message_actions: Vec<MessageAction>,

    /// Actions that must be in the message toolbar
    pub visible_message_actions: Vec<MessageAction>,
}

impl AllMessageActions {
    /// Create AllMessageActions using the unified builder pattern
    #[allow(clippy::too_many_arguments)]
    pub fn from_context(
        current_label: LabelId,
        is_unread: bool,
        is_starred: bool,
        can_reply: bool,
        can_reply_all: bool,
        theme: Option<ThemeOpts>,
        mobile_actions: &[MobileAction],
        inbox: MovableSystemFolderAction,
        archive: MovableSystemFolderAction,
        trash: MovableSystemFolderAction,
        spam: MovableSystemFolderAction,
    ) -> Self {
        // For single messages, any_* and all_* flags are the same
        let any_read = !is_unread;
        let all_read = !is_unread;
        let all_starred = is_starred;

        let context = ActionContext {
            current_label,
            any_unread: is_unread,
            any_read,
            all_read,
            any_starred: is_starred,
            all_starred,
            theme,
            folders: crate::actions::SystemFolders {
                inbox,
                archive,
                trash,
                spam,
            },
            can_reply,
            can_reply_all,
            is_conversation: false, // Messages are not conversations
        };

        let builder =
            crate::actions::MobileActionsBuilder::<MessageAction>::new(context, mobile_actions);
        let (visible_message_actions, hidden_message_actions) = builder.build();

        Self {
            hidden_message_actions,
            visible_message_actions,
        }
    }
}

/// Actions that can be taken on a message.
/// It reflects with low granularity what can be done.
/// Each of the options is meant to display a button.
/// This is the comprehensive Phase 2 message action system.
///
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageAction {
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

    // Communication
    Reply,
    ReplyAll,
    Forward,

    // Export/View
    Print,
    ViewHeaders,
    ViewHTML,
    ViewInLightMode,
    ViewInDarkMode,

    // Utility
    ReportPhishing,
    More,
}

impl MessageAction {
    fn toggle_view_mode(theme: Option<&ThemeOpts>) -> Option<Self> {
        // In light theme we do not want to have any theme-related actions
        if theme?.current_theme == MailTheme::DarkMode {
            if theme?.theme_override == Some(MailTheme::LightMode) {
                Some(Self::ViewInDarkMode)
            } else {
                Some(Self::ViewInLightMode)
            }
        } else {
            None
        }
    }

    fn is_reply_action(&self) -> bool {
        matches!(
            self,
            MessageAction::Reply | MessageAction::ReplyAll | MessageAction::Forward
        )
    }

    fn is_move_action(&self) -> bool {
        matches!(
            self,
            MessageAction::MoveTo
                | MessageAction::MoveToSystemFolder(_)
                | MessageAction::NotSpam(_)
                | MessageAction::PermanentDelete
        )
    }

    fn is_general_action(&self) -> bool {
        matches!(
            self,
            MessageAction::Print
                | MessageAction::ViewHeaders
                | MessageAction::ViewHTML
                | MessageAction::ViewInLightMode
                | MessageAction::ViewInDarkMode
                | MessageAction::ReportPhishing
        )
    }

    fn is_message_action(&self) -> bool {
        matches!(
            self,
            MessageAction::Star
                | MessageAction::Unstar
                | MessageAction::LabelAs
                | MessageAction::MarkRead
                | MessageAction::MarkUnread
        )
    }
}

// Implementation of conversion from GenericAction to MessageAction
impl From<GenericAction> for MessageAction {
    fn from(action: GenericAction) -> Self {
        match action {
            GenericAction::MarkRead => Self::MarkRead,
            GenericAction::MarkUnread => Self::MarkUnread,
            GenericAction::Star => Self::Star,
            GenericAction::Unstar => Self::Unstar,
            GenericAction::LabelAs => Self::LabelAs,
            GenericAction::MoveTo => Self::MoveTo,
            GenericAction::MoveToSystemFolder(folder) => Self::MoveToSystemFolder(folder),
            GenericAction::NotSpam(folder) => Self::NotSpam(folder),
            GenericAction::PermanentDelete => Self::PermanentDelete,
            GenericAction::More => Self::More,
        }
    }
}

// Implementation of generic mobile actions for MessageAction
impl GenericMobileActions for MessageAction {
    /// Convert MobileAction to MessageAction with context
    fn from_mobile_action(
        mobile_action: &crate::datatypes::MobileAction,
        context: &ActionContext,
    ) -> Option<Self> {
        use crate::datatypes::MobileAction::*;
        match mobile_action {
            ToggleRead => Some(Self::toggle_read(context.any_unread)),
            ToggleStar => Some(Self::toggle_star(context.any_starred)),
            Archive => Some(Self::toggle_archive(
                &context.current_label,
                &context.folders.inbox,
                &context.folders.archive,
            )),
            Trash => Some(Self::toggle_trash(
                &context.current_label,
                &context.folders.trash,
            )),
            Spam => Some(Self::toggle_spam(
                &context.current_label,
                &context.folders.inbox,
                &context.folders.spam,
            )),
            Move => Some(Self::MoveTo),
            Label => Some(Self::LabelAs),
            Reply => {
                if context.can_reply {
                    Some(Self::Reply)
                } else {
                    None
                }
            }
            Forward => {
                if context.can_reply {
                    Some(Self::Forward)
                } else {
                    None
                }
            }
            Print => Some(Self::Print),
            ViewHeaders => Some(Self::ViewHeaders),
            ViewHTML => Some(Self::ViewHTML),
            ToggleLight => Self::toggle_view_mode(context.theme.as_ref()),
            ReportPhishing => Some(Self::ReportPhishing),
            // Unsupported actions for messages
            Snooze | SaveAttachments | SavePDF | SenderEmails | Remind | Other(_) => None,
        }
    }

    /// Message-specific actions: Only communication actions that should be treated generically
    fn get_low_priority_actions(context: &ActionContext) -> Vec<Self> {
        let mut actions = vec![Self::Print, Self::ViewHeaders, Self::ViewHTML];

        if let Some(toggle_view_mode) = Self::toggle_view_mode(context.theme.as_ref()) {
            actions.push(toggle_view_mode);
        }

        actions.push(Self::ReportPhishing);

        actions
    }

    fn get_high_priority_actions(context: &ActionContext) -> Vec<Self> {
        let mut actions = Vec::new();

        // Communication actions are high priority for messages
        if context.can_reply {
            actions.push(Self::Reply);
            if context.can_reply_all {
                actions.push(Self::ReplyAll);
            }
            actions.push(Self::Forward);
        }

        actions
    }

    /// Check if two MessageActions are counter-actions
    fn are_counter_actions(action1: &Self, action2: &Self) -> bool {
        use MessageAction::*;
        matches!(
            (action1, action2),
            (MarkRead, MarkUnread)
                | (MarkUnread, MarkRead)
                | (Star, Unstar)
                | (Unstar, Star)
                | (ViewInDarkMode, ViewInLightMode)
                | (ViewInLightMode, ViewInDarkMode)
        )
    }
}
