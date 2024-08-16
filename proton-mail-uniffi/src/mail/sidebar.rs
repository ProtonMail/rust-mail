//! Structure for working with [`Sidebar`] component.
//!
//! The methods presented in this structure operate on [`Label`]s currently, but action on other
//! items could be added as needed in the future.
//!

use crate::mail::datatypes::Label;
use crate::mail::datatypes::LabelType;
use crate::mail::{MailSessionError, MailUserSession};
use crate::{LiveQueryCallback, WatchHandle};
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::LabelType as RealLabelType;
use proton_mail_common::models::Label as RealLabel;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::StashError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::spawn as spawn_async;
use tracing::debug;
use tracing::warn;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SidebarError {
    #[error("Mailbox Error: {0}")]
    MailSessionError(#[from] MailSessionError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
}

type SidebarResult<T> = Result<T, SidebarError>;

impl From<proton_mail_common::SidebarError> for SidebarError {
    fn from(error: proton_mail_common::SidebarError) -> Self {
        match error {
            proton_mail_common::SidebarError::MailContext(e) => Self::MailSessionError(e.into()),
            proton_mail_common::SidebarError::Stash(e) => Self::Stash(e),
        }
    }
}

/// A [`Sidebar`] provides a gateway to manipulating actions accessible from sidebar
#[derive(uniffi::Object)]
pub struct Sidebar {
    /// The inner sidebar, which is the real internal type.
    sidebar: proton_mail_common::Sidebar,
}

#[uniffi::export]
impl Sidebar {
    /// Create a new structure to handle sidebar.
    #[must_use]
    #[uniffi::constructor]
    pub fn new(ctx: &MailUserSession) -> Self {
        Self {
            sidebar: proton_mail_common::Sidebar::new(ctx.ctx().clone()),
        }
    }

    /// Get the list of the System Folder to display in the sidebar.
    ///
    /// That list is filtered in function of [`MailSettings::almost_all_mail`] and some are hidden
    /// when empty (`Scheduled`, `Outbox` and `Snoozed`)
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn system_labels(&self) -> SidebarResult<Vec<Label>> {
        Ok(self
            .sidebar
            .system_labels()
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// # Parameters
    ///
    /// * `parent_id` - id of the parent folder (or `None` for root folders)
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_folders(&self, parent_id: Option<u64>) -> SidebarResult<Vec<Label>> {
        Ok(self
            .sidebar
            .custom_folders(parent_id.map(Into::into))
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Get the list of Custom Labels to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_labels(&self) -> SidebarResult<Vec<Label>> {
        Ok(self
            .sidebar
            .custom_labels()
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Set folder `expanded` field to it's collapsed state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn collapse_folder(&self, local_id: u64) -> SidebarResult<()> {
        Ok(self.sidebar.collapse_folder(local_id.into()).await?)
    }

    /// Set folder `expanded` field to it's expanded state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn expand_folder(&self, local_id: u64) -> SidebarResult<()> {
        Ok(self.sidebar.expand_folder(local_id.into()).await?)
    }

    /// Watch labels of a given type.
    ///
    /// Watches labels of a specified label type for changes. When the labels
    /// change, the callback will be invoked.
    ///
    /// # Parameters
    ///
    /// * `label_type` - an enum value from `System`, `Folder` and `Label`.
    /// * `callback`   - The callback to use for updates. When the specified label
    ///                  list changes, the callback will be invoked.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    pub async fn watch_labels(
        &self,
        label_type: LabelType,
        callback: Box<dyn LiveQueryCallback>,
    ) -> SidebarResult<Arc<WatchHandle>> {
        let (sender, receiver) = flume::unbounded::<ResultsetChange<RealLabel, RealLocalId>>();
        let results = RealLabel::find(
            "WHERE label_type = ?",
            params![RealLabelType::from(label_type)],
            self.sidebar.user_ctx.stash(),
            Some(sender),
        )
        .await?;
        // Unwrapping is safe here, as we will always have the local ID
        let mut ids = results
            .iter()
            .map(|m| m.local_id.unwrap())
            .collect::<Vec<_>>();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);

        spawn_async(async move {
            while let Ok(change) = receiver.recv_async().await {
                if stop_flag_clone.load(Ordering::SeqCst) {
                    debug!("Stop flag set, stopping watch");
                    break;
                }
                match change {
                    ResultsetChange::Inserted(label) => {
                        if label.label_type == label_type.into() {
                            debug!("Received new label for watched label type ({label_type})");
                            // Unwrapping is safe here, as we will always have the local ID
                            ids.push(label.local_id.unwrap());
                            callback.on_update();
                        } else {
                            debug!("Received new label for different label type ({} instead of {label_type})", label.label_type);
                        }
                    }
                    ResultsetChange::Updated(label) => {
                        if label.label_type == label_type.into() {
                            debug!("Received updated label for watched label type ({label_type})");
                            callback.on_update();
                        } else {
                            debug!("Received updated label for different label type ({} instead of {label_type})", label.label_type);
                        }
                    }
                    ResultsetChange::Deleted(local_label_id) => {
                        if ids.contains(&local_label_id) {
                            debug!("Received deleted label for watched label type ({label_type})");
                            callback.on_update();
                        } else {
                            debug!("Received deleted label for different label type (unknown instead of {label_type})");
                        }
                    }
                    _ => {
                        warn!("Received unknown change type");
                    }
                };
            }
        });
        Ok(Arc::new(WatchHandle { stop_flag }))
    }
}
