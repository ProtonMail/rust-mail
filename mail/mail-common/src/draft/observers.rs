use crate::models::DraftSendResult;
use stash::stash::{Stash, StashError, WatcherHandle};
use std::collections::HashSet;
use tracing::error;

#[cfg(test)]
#[path = "../tests/draft/observer.rs"]
mod tests;

/// A watcher for new draft send results.
///
/// Unlike other watchers, this watcher only returns new entries that are added to the table
/// after its creation.
pub struct DraftSendResultWatcher {
    watcher_handle: WatcherHandle,
    stash: Stash,
    unseen: HashSet<DraftSendResult>,
}

impl DraftSendResultWatcher {
    /// Create a new instance with the given `stash` db pool.
    ///
    /// # Errors
    ///
    /// Returns error if the registration or initial db query failed.
    pub async fn new(stash: Stash) -> Result<Self, StashError> {
        let conn = stash.connection();

        let all_unseen = DraftSendResult::unseen(&conn).await?;

        let handle = DraftSendResult::watch(&stash)?;

        Ok(Self {
            watcher_handle: handle,
            stash,
            unseen: HashSet::from_iter(all_unseen),
        })
    }

    /// Wait on the next new send result.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed or the [`StashError::WatcherError`] if the
    /// connection to the watcher was lost.
    pub async fn next(&mut self) -> Result<Vec<DraftSendResult>, StashError> {
        loop {
            self.watcher_handle
                .receiver
                .recv_async()
                .await
                .map_err(|_| StashError::WatcherError("Connection Lost".to_owned()))?;

            let mut all_unseen = DraftSendResult::unseen(&self.stash.connection())
                .await
                .inspect_err(|e| error!("Failed to load draft send results: {e}"))?;

            if all_unseen.is_empty() {
                // Nothing to do.
                continue;
            }

            let new_state = HashSet::from_iter(all_unseen.clone());
            if new_state.difference(&self.unseen).next().is_none() {
                // no difference, continue loop
                continue;
            }

            // remove old entries
            all_unseen.retain(|v| !self.unseen.contains(v));

            // Update state
            self.unseen = new_state;

            if all_unseen.is_empty() {
                // Nothing to report.
                continue;
            }

            // return result.
            return Ok(all_unseen);
        }
    }
}
