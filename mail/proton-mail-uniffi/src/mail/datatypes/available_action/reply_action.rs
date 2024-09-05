use crate::UniffiEnum;
use proton_mail_common::actions::ReplyAction as RealReplyAction;

#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum ReplyAction {
    Reply,
    ReplyAll,
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
