pub mod contacts;
pub mod event_poll;

use crate::UserContext;
use proton_action_queue::action::{Action, FactoryError, Handler};
use proton_action_queue::queue::Queue;
use std::sync::Weak;

pub(crate) fn register_core_actions(queue: &Queue, ctx: &Weak<UserContext>) {
    fn register_action<T>(queue: &Queue, handler: T)
    where
        T: Handler,
        T::Action: Action<Handler = T>,
    {
        if let Err(e) = queue.register::<T::Action>(handler) {
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

    register_action(queue, event_poll::EventPollHandler { ctx: ctx.clone() });
    register_action(queue, contacts::DeleteHandler { ctx: ctx.clone() });
}
