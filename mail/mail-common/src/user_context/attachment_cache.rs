use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct AttachmentCacheState {
    is_cleanup_running: Arc<AtomicBool>,
}

impl AttachmentCacheState {
    pub fn new() -> Self {
        Self {
            is_cleanup_running: Default::default(),
        }
    }

    pub fn is_cleanup_running(&self) -> &Arc<AtomicBool> {
        &self.is_cleanup_running
    }
}

impl Default for AttachmentCacheState {
    fn default() -> Self {
        Self::new()
    }
}
