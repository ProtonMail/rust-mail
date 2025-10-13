use std::{collections::BTreeSet, marker::PhantomData};

use sqlite_watcher::watcher::TableObserver;

use crate::{
    orm::Model,
    stash::{Stash, StashError, WatcherHandle},
};

/// A watcher for changes to a specific database table associated with a model.
///
/// This struct implements the `TableObserver` trait to monitor changes to the table
/// defined by the model `M`. When a change is detected in the observed table, it
/// sends a notification through a flume channel to signal that the table has been
/// modified. This can be used to trigger cache invalidation, UI updates, or other
/// reactive behaviors in response to database changes.
///
/// The `TableWatcher` is designed to be lightweight and uses `PhantomData` to tie
/// the watcher to the specific model type without storing any actual data from the model.
pub struct TableWatcher<M: Model> {
    sender: flume::Sender<()>,
    typ: PhantomData<M>,
}

impl<M: Model> TableObserver for TableWatcher<M> {
    fn tables(&self) -> Vec<String> {
        vec![M::table_name().to_string()]
    }
    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                let table_name = M::table_name();
                tracing::error!(
                    "Failed to send notification for TableWatcher({table_name}): {:?}",
                    e
                );
            })
            .ok();
    }
}

impl<M: Model> TableWatcher<M> {
    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| {
                Box::new(Self {
                    sender,
                    typ: PhantomData,
                })
            })
            .await
    }
}
