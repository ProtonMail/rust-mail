use std::{collections::HashMap, sync::Weak};

use proton_core_api::services::proton::UserId;

use crate::{UserContext, app_events::OnUserContextMapChanged};

use super::services::ContextEventService;

#[derive(Default)]
pub struct ActiveUserContextMap {
    map: HashMap<UserId, Weak<UserContext>>,
}

impl ActiveUserContextMap {
    pub fn get(&self, user_id: &UserId) -> Option<&Weak<UserContext>> {
        self.map.get(user_id)
    }

    pub fn values(&self) -> impl Iterator<Item = &Weak<UserContext>> {
        self.map.values()
    }

    // IMPORTANT: Every mutable method should either publish or create a new event

    pub fn remove(&mut self, user_id: &UserId, event_service: &ContextEventService) {
        self.map.remove(user_id);
        event_service.publish(OnUserContextMapChanged);
    }

    pub fn insert(
        &mut self,
        user_id: UserId,
        context: Weak<UserContext>,
        event_service: &ContextEventService,
    ) {
        self.map.insert(user_id, context);
        event_service.publish(OnUserContextMapChanged);
    }

    #[must_use]
    pub fn cleanup_dropped(&mut self) -> Option<OnUserContextMapChanged> {
        let mut changed = false;

        self.map.retain(|_, value| {
            let should_stay = value.strong_count() != 0;
            changed |= !should_stay;
            should_stay
        });

        changed.then_some(OnUserContextMapChanged)
    }
}
