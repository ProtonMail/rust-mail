use crate::UniffiEnum;
use proton_mail_common::actions::ReplyAction as RealReplyAction;

/// This enum represents the action of replying to a message.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum ReplyAction {
    /// Static, Reply action is always available.
    Reply,

    /// If the item has multiple recipients, ReplyAll action is available.
    ReplyAll,

    /// Static, Forward action is always available.
    Forward,
}

impl From<RealReplyAction> for ReplyAction {
    fn from(value: RealReplyAction) -> Self {
        match value {
            RealReplyAction::Reply => ReplyAction::Reply,
            RealReplyAction::ReplyAll => ReplyAction::ReplyAll,
            RealReplyAction::Forward => ReplyAction::Forward,
        }
    }
}
