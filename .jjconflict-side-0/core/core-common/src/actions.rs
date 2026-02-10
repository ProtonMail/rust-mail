pub mod contacts;
pub mod dependency_builder;
pub mod event_poll;
pub mod user_feature_flags;

use crate::{Origin, UserContext};
use proton_action_queue::action::{Action, FactoryError, Handler};
use proton_action_queue::queue::Queue;
use proton_core_api::session::Session;
use stash::UserDb;
use std::sync::Weak;

pub(crate) fn register_actions(
    origin: Origin,
    queue: &Queue<UserDb>,
    ctx: &Weak<UserContext>,
    api: &Session,
) {
    fn reg<T>(queue: &Queue<UserDb>, handler: T)
    where
        T: Handler<UserDb>,
        T::Action: Action<UserDb, Handler = T>,
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
            reg(
                queue,
                user_feature_flags::OverrideFlagHandler { api: api.clone() },
            );
        }

        Origin::ShareExt => {
            //
        }
    }
}
