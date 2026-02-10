use crate::models::InitializationWatcher;
use std::sync::Arc;

/// For main app only
pub struct InitializationService {
    initialization_watcher: Arc<InitializationWatcher>,
}

impl InitializationService {
    #[must_use]
    pub fn new(initialization_watcher: Arc<InitializationWatcher>) -> Self {
        Self {
            initialization_watcher,
        }
    }

    #[must_use]
    pub fn initialization_watcher(&self) -> &Arc<InitializationWatcher> {
        &self.initialization_watcher
    }
}
