use crate::mail::datatypes::labels::custom_folder::SidebarCustomFolder;
use crate::mail::datatypes::labels::custom_labels::SidebarCustomLabel;
use crate::mail::{MailSessionError, MailUserSession};
use crate::uniffi_async;
use proton_core_common::datatypes::LabelId as RealLabelId;
use proton_mail_common::datatypes::labels::custom_folder::CustomFolder as RealCustomFolder;
use proton_mail_common::datatypes::labels::custom_labels::CustomLabel as RealCustomLabel;
use proton_mail_common::datatypes::{LabelType as RealLabelType, SystemLabelId};
use proton_mail_common::models::Label as RealLabel;
use stash::orm::Model;
use stash::params;

#[uniffi::export]
impl MailUserSession {
    /// Return the list of labels of type Folder into which a conversations or
    /// message can be moved.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub async fn movable_folders(&self) -> Result<Vec<SidebarCustomFolder>, MailSessionError> {
        let stash = self.ctx().user_stash().clone();
        uniffi_async(async move {
            // TODO: Unclear how exactly the system folders fit into this.
            let _sys_folders = RealLabelId::movable_sys_folder_list();
            let labels = RealLabel::find(
                "WHERE label_type = ? ORDER BY display_order",
                params![RealLabelType::Folder],
                &stash,
                None,
            )
            .await?;
            let labels = RealCustomFolder::from_labels(labels.as_slice(), &stash).await?;
            Ok(labels.into_iter().map(SidebarCustomFolder::from).collect())
        })
        .await
    }

    /// Return the list of labels of type Label that can be applied to conversations or
    /// messages.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub async fn applicable_labels(&self) -> Result<Vec<SidebarCustomLabel>, MailSessionError> {
        let stash = self.ctx.user_stash().clone();
        uniffi_async(async move {
            let labels = RealLabel::find(
                "WHERE label_type = ? ORDER BY display_order",
                params![RealLabelType::Label],
                &stash,
                None,
            )
            .await?;
            let labels = RealCustomLabel::from_labels(labels.as_slice(), &stash).await?;
            Ok(labels.into_iter().map(SidebarCustomLabel::from).collect())
        })
        .await
    }
}
