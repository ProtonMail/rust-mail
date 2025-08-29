pub mod contacts;
pub mod dependency_builder;
pub mod event_poll;

use crate::{Origin, UserContext};
use proton_action_queue::action::{FactoryError, Handler};
use proton_action_queue::queue::Queue;
use proton_core_api::session::Session;
use std::sync::Weak;

pub(crate) fn register_actions(
    origin: Origin,
    queue: &Queue,
    ctx: &Weak<UserContext>,
    api: &Session,
) {
    fn reg<T>(queue: &Queue, handler: T)
    where
        T: Handler,
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

    match origin {
        Origin::App => {
            reg(queue, event_poll::EventPollHandler { ctx: ctx.clone() });
            reg(queue, contacts::DeleteHandler { api: api.clone() });
        }

        Origin::ShareExt => {
            //
        }
    }
}
