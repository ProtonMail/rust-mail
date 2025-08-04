use crate::event_loop::EventLoopActionIds;
use proton_event_loop::EventPoll;
use std::sync::Arc;
use tokio::sync::Mutex;

/// For main application use only.
pub struct EventLoopService {
    event_loop: EventPoll,
    last_event_loop_action_ids: Arc<Mutex<EventLoopActionIds>>,
}

impl EventLoopService {
    pub fn new(event_loop: EventPoll) -> Self {
        Self {
            event_loop,
            last_event_loop_action_ids: Arc::new(Mutex::new(EventLoopActionIds::default())),
        }
    }

    pub fn event_loop(&self) -> &EventPoll {
        &self.event_loop
    }

    pub fn last_event_loop_action_ids(&self) -> &Arc<Mutex<EventLoopActionIds>> {
        &self.last_event_loop_action_ids
    }
}
