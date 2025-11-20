use crate::errors::UserSessionError;
use crate::mail::MailUserSession;
use crate::mail::datatypes::labels::custom_folder::SidebarCustomFolder;
use crate::mail::datatypes::labels::custom_labels::SidebarCustomLabel;
use crate::uniffi_async;
use proton_core_api::services::proton::LabelId as RealLabelId;
use proton_core_common::datatypes::LabelType as RealLabelType;
use proton_core_common::models::Label as RealLabel;
use proton_core_common::utils::MapVec as _;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::datatypes::labels::custom_folder::CustomFolder as RealCustomFolder;
use proton_mail_common::datatypes::labels::custom_labels::CustomLabel as RealCustomLabel;

#[uniffi_export]
impl MailUserSession {
    /// Return the list of labels of type Folder into which a conversations or
    /// message can be moved.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub async fn movable_folders(&self) -> Result<Vec<SidebarCustomFolder>, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            // TODO: Unclear how exactly the system folders fit into this.
            let _sys_folders = RealLabelId::movable_sys_folder_list();
            let tether = ctx.user_stash().connection().await?;
            let labels = RealLabel::find_by_kind(RealLabelType::Folder, &tether).await?;
            let labels = RealCustomFolder::from_labels(labels.as_slice(), &tether).await?;
            Ok::<_, RealProtonMailError>(labels.map_vec())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Return the list of labels of type Label that can be applied to conversations or
    /// messages.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub async fn applicable_labels(&self) -> Result<Vec<SidebarCustomLabel>, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let tether = ctx.user_stash().connection().await?;
            let labels = RealLabel::find_by_kind(RealLabelType::Label, &tether).await?;
            let labels = RealCustomLabel::from_labels(labels.as_slice(), &tether).await?;
            Ok::<_, RealProtonMailError>(labels.map_vec())
        })
        .await
        .map_err(UserSessionError::from)
    }
}
