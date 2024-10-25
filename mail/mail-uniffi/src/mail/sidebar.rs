//! Structure for working with [`Sidebar`] component.
//!
//! The methods presented in this structure operate on [`Label`]s currently, but action on other
//! items could be added as needed in the future.
//!

use crate::core::datatypes::Id;
use crate::errors::user_actions::{UserActionError, VoidUserActionResult};
use crate::mail::datatypes::labels::custom_folder::SidebarCustomFolder;
use crate::mail::datatypes::labels::custom_labels::SidebarCustomLabel;
use crate::mail::datatypes::labels::system_labels::SidebarSystemLabel;
use crate::mail::datatypes::LabelType;
use crate::mail::MailUserSession;
use crate::utils::damp;
use crate::{spawn_async, uniffi_async, LiveQueryCallback, WatchHandle};
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::LabelType as RealLabelType;
use proton_mail_common::errors::user_actions::UserActionError as RealUserActionError;
use proton_mail_common::models::Label as RealLabel;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::debug;
use tracing::warn;

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

    /// Set folder `expanded` field to it's collapsed state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn collapse_folder(&self, local_id: Id) -> VoidUserActionResult {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            Result::<_, RealUserActionError>::Ok(sidebar.collapse_folder(local_id.into()).await?)
        })
        .await
        .into()
    }

    /// Set folder `expanded` field to it's expanded state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn expand_folder(&self, local_id: Id) -> VoidUserActionResult {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            Result::<_, RealUserActionError>::Ok(sidebar.expand_folder(local_id.into()).await?)
        })
        .await
        .into()
    }
}

#[proton_uniffi_macros::export_result]
impl Sidebar {
    /// Get the list of the System Folder to display in the sidebar.
    ///
    /// That list is filtered in function of [`MailSettings::almost_all_mail`] and some are hidden
    /// when empty (`Scheduled`, `Outbox` and `Snoozed`)
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn system_labels(&self) -> Result<Vec<SidebarSystemLabel>, UserActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.system_labels().await?;
            Result::<_, RealUserActionError>::Ok(
                labels.into_iter().map(SidebarSystemLabel::from).collect(),
            )
        })
        .await
        .map_err(Into::into)
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, UserActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.custom_folders().await?;
            Result::<_, RealUserActionError>::Ok(
                labels.into_iter().map(SidebarCustomFolder::from).collect(),
            )
        })
        .await
        .map_err(Into::into)
    }

    /// Get the list of all the Custom Folders.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn all_custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, UserActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.all_custom_folders().await?;
            Result::<_, RealUserActionError>::Ok(
                labels.into_iter().map(SidebarCustomFolder::from).collect(),
            )
        })
        .await
        .map_err(Into::into)
    }

    /// Get the list of Custom Labels to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_labels(&self) -> Result<Vec<SidebarCustomLabel>, UserActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.custom_labels().await?;
            Result::<_, RealUserActionError>::Ok(
                labels.into_iter().map(SidebarCustomLabel::from).collect(),
            )
        })
        .await
        .map_err(Into::into)
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
    ) -> Result<Arc<WatchHandle>, UserActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let (sender, receiver) = flume::unbounded::<ResultsetChange<RealLabel, RealLocalId>>();
            let results = RealLabel::find(
                "WHERE label_type = ?",
                params![RealLabelType::from(label_type)],
                sidebar.user_ctx.user_stash(),
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
                let callback = damp(callback);
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
                                callback();
                            } else {
                                debug!("Received new label for different label type ({} instead of {label_type})", label.label_type);
                            }
                        }
                        ResultsetChange::Updated(label) => {
                            if label.label_type == label_type.into() {
                                debug!("Received updated label for watched label type ({label_type})");
                                callback();
                            } else {
                                debug!("Received updated label for different label type ({} instead of {label_type})", label.label_type);
                            }
                        }
                        ResultsetChange::Deleted(local_label_id) => {
                            if ids.contains(&local_label_id) {
                                debug!("Received deleted label for watched label type ({label_type})");
                                callback();
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
            Result::<_, RealUserActionError>::Ok(Arc::new(WatchHandle { stop_flag }))
        }).await.map_err(Into::into)
    }
}
