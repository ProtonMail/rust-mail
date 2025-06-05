use proton_action_queue::action::ActionId;
use std::time::Duration;

pub mod subscriber;

// Re-export common macros for easier access
pub use subscriber::macros::*;

/// Defines how the event loop should be polled
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum EventPollMode {
    /// On demand,
    Manual,
    /// Background task that queues a request to polls the event loop in the
    /// specified duration.
    Automatic(Duration),
}

#[derive(Debug, Default)]
pub struct EventLoopActionIds {
    pub last_event_loop_action_id: Option<ActionId>,
    pub last_rollback_action_id: Option<ActionId>,
}
