//! Structure for working with [`Sidebar`] component.
//!
//! The methods presented in this structure operate on [`Label`]s currently, but action on other
//! items could be added as needed in the future.
//!

use crate::core::datatypes::Id;
use crate::errors::{ActionError, VoidActionResult};
use crate::mail::datatypes::labels::custom_folder::SidebarCustomFolder;
use crate::mail::datatypes::labels::custom_labels::SidebarCustomLabel;
use crate::mail::datatypes::labels::system_labels::SidebarSystemLabel;
use crate::mail::datatypes::LabelType;
use crate::mail::MailUserSession;
use crate::utils::damp;
use crate::{async_runtime, spawn_async, uniffi_async, LiveQueryCallback, WatchHandle};
use itertools::Itertools;
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::LabelType as RealLabelType;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::Label as RealLabel;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::debug;

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
    pub async fn collapse_folder(&self, local_id: Id) -> VoidActionResult {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(sidebar.collapse_folder(local_id.into()).await?)
        })
        .await
        .map_err(ActionError::from)
        .into()
    }

    /// Set folder `expanded` field to it's expanded state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn expand_folder(&self, local_id: Id) -> VoidActionResult {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            Result::<_, RealProtonMailError>::Ok(sidebar.expand_folder(local_id.into()).await?)
        })
        .await
        .map_err(ActionError::from)
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
    pub async fn system_labels(&self) -> Result<Vec<SidebarSystemLabel>, ActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.system_labels().await?;
            Result::<_, RealProtonMailError>::Ok(
                labels.into_iter().map(SidebarSystemLabel::from).collect(),
            )
        })
        .await
        .map_err(ActionError::from)
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, ActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.custom_folders().await?;
            Result::<_, RealProtonMailError>::Ok(
                labels.into_iter().map(SidebarCustomFolder::from).collect(),
            )
        })
        .await
        .map_err(ActionError::from)
    }

    /// Get the list of all the Custom Folders.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn all_custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, ActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.all_custom_folders().await?;
            Result::<_, RealProtonMailError>::Ok(
                labels.into_iter().map(SidebarCustomFolder::from).collect(),
            )
        })
        .await
        .map_err(ActionError::from)
    }

    /// Get the list of Custom Labels to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_labels(&self) -> Result<Vec<SidebarCustomLabel>, ActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let labels = sidebar.custom_labels().await?;
            Result::<_, RealProtonMailError>::Ok(
                labels.into_iter().map(SidebarCustomLabel::from).collect(),
            )
        })
        .await
        .map_err(ActionError::from)
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
    ) -> Result<Arc<WatchHandle>, ActionError> {
        let sidebar = self.sidebar.clone();
        uniffi_async(async move {
            let (sender, receiver) = flume::unbounded::<ResultsetChange<RealLabel, RealLocalId>>();
            let tether = sidebar.user_ctx.user_stash().connection();
            let results = RealLabel::find(
                "WHERE label_type = ?",
                params![RealLabelType::from(label_type)],
                &tether,
                Some(sender),
            )
            .await?;
            // Unwrapping is safe here, as we will always have the local ID
            let mut ids = results.iter().map(|m| m.local_id.unwrap()).collect_vec();
            let stop_flag = Arc::new(AtomicBool::new(false));
            let weak_stop_flag = Arc::downgrade(&stop_flag);

            spawn_async(async move {
                let callback = damp(callback).await;

                let callback = Arc::new(callback);
                while let Ok(change) = receiver.recv_async().await {
                    let callback = callback.clone();
                    let callback = move || callback();
                    let Some(stop_flag) = weak_stop_flag.upgrade() else {
                        debug!("Watch handle dropped, stopping watch");
                        break;
                    };

                    if stop_flag.load(Ordering::SeqCst) {
                        debug!("Stop flag set, stopping watch");
                        break;
                    }
                    match change {
                        ResultsetChange::Inserted(label) => {
                            if label.label_type == label_type.into() {
                                // Unwrapping is safe here, as we will always have the local ID
                                ids.push(label.local_id.unwrap());
                                _ = async_runtime().spawn_blocking(callback).await;
                            }
                        }
                        ResultsetChange::Updated(label) => {
                            if label.label_type == label_type.into() {
                                _ = async_runtime().spawn_blocking(callback).await;
                            }
                        }
                        ResultsetChange::Deleted(local_label_id) => {
                            if ids.contains(&local_label_id) {
                                _ = async_runtime().spawn_blocking(callback).await;
                            }
                        }
                        _ => {}
                    };
                }
            });
            Result::<_, RealProtonMailError>::Ok(Arc::new(WatchHandle { stop_flag }))
        })
        .await
        .map_err(ActionError::from)
    }
}
