use proton_action_queue::action::ActionDependencyKey;

pub mod block;
pub mod unblock;
pub mod update_incoming_defaults;

fn incoming_defaults_dependency_key(email: &str) -> ActionDependencyKey {
    ActionDependencyKey::from(format!("incoming-default-{email}"))
}
