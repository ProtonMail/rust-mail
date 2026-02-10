use sqlite_watcher::watcher::TableObserver;
use std::collections::BTreeSet;
use tracing::error;

pub struct MailScrollerWatcher {
    pub(super) sender: flume::Sender<()>,
    pub(super) tables: Vec<String>,
}

impl TableObserver for MailScrollerWatcher {
    fn tables(&self) -> Vec<String> {
        self.tables.clone()
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                error!(
                    "Failed to send notification for MailScrollerWatcher: {:?}",
                    e
                );
            })
            .ok();
    }
}
