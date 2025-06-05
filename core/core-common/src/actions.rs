use proton_action_queue::action::{Action, FactoryError};
use proton_action_queue::queue::Queue;

pub mod contacts;
pub mod event_poll;

pub(crate) fn register_core_actions(queue: &Queue) {
    fn register_action<T: Action>(queue: &Queue) {
        if let Err(e) = queue.register::<T>() {
            match e {
                FactoryError::AlreadyRegistered(_) => {
                    // Do nothing it is possible we already registered this action
                    // in the queue once before.
                }
                e => {
                    panic!("Failed to register action: {e:?}");
                }
            }
        }
    }

    register_action::<event_poll::EventPoll>(queue);
    register_action::<contacts::Delete>(queue);
}
