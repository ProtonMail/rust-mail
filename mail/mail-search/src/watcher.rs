//! Table watcher for search index intents
//!
//! Watches the `search_index_intents` table for changes and notifies the worker
//! when new intents are available. This solves both the multi-account support
//! issue and the race condition (watcher only fires after transaction commits).

use sqlite_watcher::watcher::TableObserver;
use stash::stash::{Stash, StashError, WatcherHandle};
use std::collections::BTreeSet;

/// Table name for search index intents
const SEARCH_INDEX_INTENTS_TABLE: &str = "search_index_intents";

/// Watcher for search index intents table
pub struct SearchIndexIntentWatcher;

impl SearchIndexIntentWatcher {
    /// Create a watcher for the `search_index_intents` table
    ///
    /// Returns a `WatcherHandle` that can be used to receive notifications
    /// when the table changes. The watcher automatically detects which
    /// Stash instance (account) the change belongs to.
    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(SearchIndexIntentTableWatcher { sender }))
            .await
    }
}

struct SearchIndexIntentTableWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for SearchIndexIntentTableWatcher {
    fn tables(&self) -> Vec<String> {
        vec![SEARCH_INDEX_INTENTS_TABLE.to_string()]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for SearchIndexIntentWatcher: {:?}",
                    e
                );
            })
            .ok();
    }
}
