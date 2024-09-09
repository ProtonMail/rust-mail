/// This enum represents the action of replying to a message.
///
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyAction {
    /// Static, Reply action is always available.
    Reply,

    /// If the item has multiple recipients, ReplyAll action is available.
    ReplyAll,

    /// Static, Forward action is always available.
    Forward,
}

impl ReplyAction {
    /// Returns a list of actions that can be performed on an item with single recipient.
    ///
    pub fn single_address() -> Vec<Self> {
        vec![ReplyAction::Reply, ReplyAction::Forward]
    }

    pub fn all() -> Vec<Self> {
        vec![
            ReplyAction::Reply,
            ReplyAction::ReplyAll,
            ReplyAction::Forward,
        ]
    }
}
