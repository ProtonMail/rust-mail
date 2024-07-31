use crate::mail::datatypes::{Label, LabelType};
use crate::mail::{MailSessionError, MailUserSession};
use proton_core_common::datatypes::LabelId as RealLabelId;
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
    pub async fn movable_folders(&self) -> Result<Vec<Label>, MailSessionError> {
        // TODO: Unclear how exactly the system folders fit into this.
        let _sys_folders = RealLabelId::movable_sys_folder_list();
        Ok(RealLabel::find(
            "WHERE label_type = ? ORDER BY display_order",
            params![RealLabelType::from(LabelType::Folder)],
            self.ctx().stash(),
            None,
        )
        .await
        .map(|labels| labels.into_iter().map(Label::from).collect())?)
    }

    /// Return the list of labels of type Label that can be applied to conversations or
    /// messages.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub async fn applicable_labels(&self) -> Result<Vec<Label>, MailSessionError> {
        Ok(RealLabel::find(
            "WHERE label_type = ? ORDER BY display_order",
            params![RealLabelType::from(LabelType::Label)],
            self.ctx().stash(),
            None,
        )
        .await
        .map(|labels| labels.into_iter().map(Label::from).collect())?)
    }
}
