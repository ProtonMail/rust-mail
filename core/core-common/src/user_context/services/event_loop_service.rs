use crate::event_loop::EventLoopActionIds;
use proton_action_queue::action::ActionId;
use proton_event_loop::EventPoll;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EventLoopService {
    event_poll: EventPoll,
    last_event_loop_action_ids: Arc<Mutex<EventLoopActionIds>>,
}

impl EventLoopService {
    #[must_use]
    pub fn new(event_loop: EventPoll, last_event_loop_action_id: Option<ActionId>) -> Self {
        Self {
            event_poll: event_loop,
            last_event_loop_action_ids: Arc::new(Mutex::new(EventLoopActionIds {
                last_event_loop_action_id,
                last_rollback_action_id: None,
            })),
        }
    }

    #[must_use]
    pub fn event_poll(&self) -> &EventPoll {
        &self.event_poll
    }

    #[must_use]
    pub fn last_event_loop_action_ids(&self) -> &Arc<Mutex<EventLoopActionIds>> {
        &self.last_event_loop_action_ids
    }
}
