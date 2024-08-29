#[derive(Debug, Clone, PartialEq)]
pub enum ReplyAction {
    Reply,
    ReplyAll,
    Forward,
}

impl ReplyAction {
    pub fn all() -> Vec<Self> {
        vec![
            ReplyAction::Reply,
            ReplyAction::ReplyAll,
            ReplyAction::Forward,
        ]
    }
}
