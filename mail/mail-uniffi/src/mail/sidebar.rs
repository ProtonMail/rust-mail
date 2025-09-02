use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ActionError, ProtonError, VoidActionResult};
use crate::mail::MailUserSession;
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

#[derive(uniffi::Object)]
pub struct Sidebar {
    ctx: MailUserContextPtr,
}

impl Sidebar {
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}

#[uniffi_export]
impl Sidebar {
    #[uniffi::constructor]
    pub fn new(session: &MailUserSession) -> Arc<Sidebar> {
        let ctx = session.ptr();

        Arc::new(Sidebar { ctx })
    }

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
    pub async fn system_labels(&self) -> Result<Vec<SidebarSystemLabel>, ActionError> {
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection().await?;
            let labels = RealSidebar.system_labels(&tether).await?;

            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
        })
        .await
        .map_err(ActionError::from)
    }

    pub async fn custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, ActionError> {
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection().await?;
            let labels = RealSidebar.custom_folders(&tether).await?;

            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
        })
        .await
        .map_err(ActionError::from)
    }

    pub async fn all_custom_folders(&self) -> Result<Vec<SidebarCustomFolder>, ActionError> {
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection().await?;
            let labels = RealSidebar.all_custom_folders(&tether).await?;

            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
        })
        .await
        .map_err(ActionError::from)
    }

    pub async fn custom_labels(&self) -> Result<Vec<SidebarCustomLabel>, ActionError> {
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let tether = stash.connection().await?;
            let labels = RealSidebar.custom_labels(&tether).await?;

            Result::<_, RealProtonMailError>::Ok(labels.map_vec())
        })
        .await
        .map_err(ActionError::from)
    }

    pub async fn watch_labels(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, ActionError> {
        let ctx = self.ctx()?;
        let stash = self.user_stash()?;

        uniffi_async(async move {
            let handle = RealLabelWithCounters::watch(&stash)?;
            let handle = watch_channel(&*ctx, handle, callback);

            Result::<_, RealProtonMailError>::Ok(handle)
        })
        .await
        .map_err(ActionError::from)
    }
}
