use super::Service;
use crate::{CoreContextError, event_loop::EventPollMode};
use async_trait::async_trait;

pub struct EventPollConfigService {
    mode: EventPollMode,
}

impl EventPollConfigService {
    #[must_use]
    pub fn new(mode: EventPollMode) -> Self {
        Self { mode }
    }

    #[must_use]
    pub fn mode(&self) -> EventPollMode {
        self.mode
    }

    #[must_use]
    pub fn is_manual(&self) -> bool {
        matches!(self.mode, EventPollMode::Manual)
    }

    #[must_use]
    pub fn is_automatic(&self) -> bool {
        matches!(self.mode, EventPollMode::Automatic(_))
    }

    #[must_use]
    pub fn automatic_interval(&self) -> Option<std::time::Duration> {
        match self.mode {
            EventPollMode::Automatic(duration) => Some(duration),
            EventPollMode::Manual => None,
        }
    }
}

#[async_trait]
impl Service for EventPollConfigService {
    type Error = CoreContextError;
}
