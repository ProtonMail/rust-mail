use crate::models::{
    DraftAttachmentMetadata, DraftAttachmentUploadState, DraftSendResult, MetadataId,
};
use proton_mail_ids::LocalAttachmentId;
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
                .inspect_err(|e| error!("Failed to load draft send results: {e:?}"))?;

            if all_unseen.is_empty() {
                // Nothing to do.
                continue;
            }

            let new_state = HashSet::from_iter(all_unseen.clone());
            if new_state == self.unseen {
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

/// Observe attachment state for a given draft.
pub struct DraftAttachmentObserver {
    id: MetadataId,
    stash: Stash,
    current: HashSet<DraftAttachmentMetadataObserverState>,
    watcher_handle: WatcherHandle,
}

impl DraftAttachmentObserver {
    /// Create new instance for the given `metadata_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn new(metadata_id: MetadataId, stash: Stash) -> Result<Self, StashError> {
        let conn = stash.connection();

        let current = DraftAttachmentMetadata::find_by_metadata_id(metadata_id, &conn).await?;

        let handle = DraftAttachmentMetadata::watch(&stash)?;

        Ok(Self {
            id: metadata_id,
            stash,
            current: HashSet::from_iter(
                current
                    .into_iter()
                    .filter(|v| !v.deleted)
                    .map(DraftAttachmentMetadataObserverState::from),
            ),
            watcher_handle: handle,
        })
    }

    /// Wait on the next update for this watcher
    ///
    /// # Errors
    ///
    /// Returns error if
    pub async fn next(&mut self) -> Result<(), StashError> {
        loop {
            self.watcher_handle
                .receiver
                .recv_async()
                .await
                .map_err(|_| StashError::WatcherError("Connection Lost".to_owned()))?;

            let conn = self.stash.connection();
            let current = DraftAttachmentMetadata::find_by_metadata_id(self.id, &conn).await?;
            let new_state_set = HashSet::from_iter(
                current
                    .into_iter()
                    .filter(|v| !v.deleted)
                    .map(DraftAttachmentMetadataObserverState::from),
            );

            // No changes continue;
            if new_state_set == self.current {
                continue;
            }

            self.current = new_state_set;
            return Ok(());
        }
    }
}

/// Custom type to track changes, not all table changes need to be reported.
#[derive(Debug, Eq, PartialEq, Hash)]
struct DraftAttachmentMetadataObserverState {
    attachment_id: LocalAttachmentId,
    state: DraftAttachmentUploadState,
}

impl From<DraftAttachmentMetadata> for DraftAttachmentMetadataObserverState {
    fn from(metadata: DraftAttachmentMetadata) -> Self {
        Self {
            attachment_id: metadata.local_attachment_id,
            state: metadata.state(),
        }
    }
}
