use crate::mail::{MailSessionError, MailUserSession};
use proton_mail_common::db::Label;
use std::sync::Arc;

#[uniffi::export]
impl MailUserSession {
    /// Return the list of labels of type Folder into which a conversations or
    /// message can be moved.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub fn movable_folders(&self) -> Result<Vec<Label>, MailSessionError> {
        Ok(self.ctx.movable_folders()?)
    }

    /// Return the list of labels of type Label that can be applied to conversations or
    /// messages.
    ///
    /// # Errors
    /// Returns an error if the list can not be retrieved.
    pub fn applicable_labels(&self) -> Result<Vec<Label>, MailSessionError> {
        Ok(self.ctx.get_labels_by_type(LabelType::Label)?)
    }
}
