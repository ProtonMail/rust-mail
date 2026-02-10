use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use tokio::task::{AbortHandle, JoinHandle};

pub struct WatchHandle {
    watch_handle: DropRemoveTableObserverHandle,
    task_handle: AbortHandle,
}

impl WatchHandle {
    #[must_use]
    pub fn new<T: Send + 'static>(
        watch_handle: DropRemoveTableObserverHandle,
        task_handle: &JoinHandle<T>,
    ) -> Self {
        Self {
            watch_handle,
            task_handle: task_handle.abort_handle(),
        }
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl WatchHandle {
    pub fn disconnect(&self) {
        self.task_handle.abort();
        self.watch_handle.unsubscribe().ok();
    }
}
