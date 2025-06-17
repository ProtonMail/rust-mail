//! Structure for working with [`Sidebar`] component.
//!
//! The methods presented in this structure operate on [`Label`]s currently, but action on other
//! items could be added as needed in the future.
//!

use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ActionError, ProtonError, VoidActionResult};
use crate::mail::MailUserSession;
use crate::mail::datatypes::LabelType;
use crate::mail::datatypes::labels::custom_folder::SidebarCustomFolder;
use crate::mail::datatypes::labels::custom_labels::SidebarCustomLabel;
use crate::mail::datatypes::labels::system_labels::SidebarSystemLabel;
use crate::mail::state::MailUserContextPtr;
use crate::{LiveQueryCallback, WatchHandle, uniffi_async, watch_channel};
use proton_core_common::utils::MapVec as _;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::LabelWithCounters as RealLabelWithCounters;
use proton_mail_common::{MailUserContext, Sidebar as RealSidebar};
use stash::stash::Stash;
use std::sync::Arc;

/// A [`Sidebar`] provides a gateway to manipulating actions accessible from sidebar
#[derive(uniffi::Object)]
pub struct Sidebar {
    /// The mail user context relevant for the sidebar.
    ctx: MailUserContextPtr,
}

impl Sidebar {
    /// Get a strong reference to the inner user context.
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    /// Get the connection to the user database
    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}

#[uniffi_export]
impl Sidebar {
    /// Create a new structure to handle sidebar.
    #[uniffi::constructor]
    pub fn new(session: &MailUserSession) -> Arc<Sidebar> {
        let ctx = session.ptr();

        Arc::new(Sidebar { ctx })
    }

    /// Set folder `expanded` field to it's collapsed state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    #[returns(VoidActionResult)]
    pub async fn collapse_folder(&self, local_id: Id) -> Result<(), ActionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            RealSidebar.collapse_folder(&ctx, local_id.into()).await?;

            Result::<_, RealProtonMailError>::Ok(())
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
    #[returns(VoidActionResult)]
    pub async fn expand_folder(&self, local_id: Id) -> Result<(), ActionError> {
        let ctx = self.ctx()?;

        uniffi_async(async move {
            RealSidebar.expand_folder(&ctx, local_id.into()).await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(ActionError::from)
        .into()
    }
}

#[uniffi_export]
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
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection();
            let labels = RealSidebar.system_labels(&tether).await?;
            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
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
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection();
            let labels = RealSidebar.custom_folders(&tether).await?;
            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
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
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection();
            let labels = RealSidebar.all_custom_folders(&tether).await?;
            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
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
        let stash = self.user_stash()?;
        uniffi_async(async move {
            let tether = stash.connection();
            let labels = RealSidebar.custom_labels(&tether).await?;
            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
        })
        .await
        .map_err(ActionError::from)
    }

    /// Watch labels of a given type.
    ///
    /// Watches labels of a specified label type for changes. When the labels
    /// change, the callback will be invoked.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    #[allow(unused_variables)]
    pub async fn watch_labels(
        &self,
        label_type: LabelType,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ActionError> {
        let ctx = self.ctx()?;
        let stash = self.user_stash()?;
        uniffi_async(async move {
            let handle = RealLabelWithCounters::watch(&stash)?;
            let handle = watch_channel(ctx, handle, callback);

            Result::<_, RealProtonMailError>::Ok(handle)
        })
        .await
        .map_err(ActionError::from)
    }
}
