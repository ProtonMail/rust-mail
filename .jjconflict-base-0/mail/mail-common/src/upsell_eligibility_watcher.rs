use proton_core_common::models::{User, UserSettings};
use sqlite_watcher::watcher::TableObserver;
use stash::UserDb;
use stash::orm::Model;
use stash::stash::{Stash, StashError, WatcherHandle};
use std::collections::BTreeSet;

pub struct UpsellEligibilityWatcher;

impl UpsellEligibilityWatcher {
    pub async fn watch(stash: &Stash<UserDb>) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(UpsellEligibilityTableWatcher { sender }))
            .await
    }
}

struct UpsellEligibilityTableWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for UpsellEligibilityTableWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            User::table_name().to_string(),
            UserSettings::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for UpsellEligibilityWatcher: {:?}",
                    e
                )
            })
            .ok();
    }
}
