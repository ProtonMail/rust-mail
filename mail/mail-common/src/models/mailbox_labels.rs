#![allow(async_fn_in_trait)]
#![allow(clippy::module_inception)]

#[cfg(test)]
#[path = "../tests/models/mailbox_labels.rs"]
mod mailbox_labels;

use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LabelType;
use proton_core_common::models::ModelExtension;
use proton_core_common::{datatypes::LocalLabelId, models::Label};
use stash::stash::Tether;
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError},
};

use crate::datatypes::{MessageRecipientDisplayMode, SystemLabelId, ViewMode};

use super::MailSettings;

/// Mailbox labels is an extension over labels, specific for mailbox only.
/// That allows us to keep labels in core-common
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("mailbox_labels")]
pub struct MailboxLabels {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub initialized: bool,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MailboxLabels {
    /// Constructor - note: [`MailboxLabels`] does not implement [`Default`] trait
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            initialized: false,
            row_id: Default::default(),
        }
    }

    /// Save mailbox labels to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to ensure
    /// that if the mailbox label already exists it is updated, and not inserted with a conflict.
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    /// * `tx` - transaction used to modify DB
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.row_id.is_none() {
            if let Some(existing) = Self::find_by_id(self.local_label_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }
        <Self as Model>::save(self, bond).await
    }
}

pub trait MailLabel {
    /// Return the preferred view mode for label.
    ///
    /// If this function returns [`None`] we should use the [`ViewMode`] defined
    /// in the user's [`MailSettings`], otherwise the returned value should be
    /// used.
    ///
    async fn view_mode(&self, tether: &Tether) -> Result<ViewMode, StashError>;

    fn is_movable_folder(&self) -> bool;

    fn recipient_display_mode(&self) -> MessageRecipientDisplayMode;
}

impl MailLabel for Label {
    async fn view_mode(&self, tether: &Tether) -> Result<ViewMode, StashError> {
        if let Some(remote_id) = self.remote_id.as_ref() {
            if *remote_id == LabelId::drafts()
                || *remote_id == LabelId::sent()
                || *remote_id == LabelId::all_drafts()
                || *remote_id == LabelId::all_sent()
                || *remote_id == LabelId::all_scheduled()
                || *remote_id == LabelId::outbox()
            {
                return Ok(ViewMode::Messages);
            }
        }
        Ok(MailSettings::get_or_default(tether).await.view_mode)
    }

    fn is_movable_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self
                .remote_id
                .as_ref()
                .is_some_and(|rid| LabelId::movable_sys_folder_list().contains(rid))
    }

    fn recipient_display_mode(&self) -> MessageRecipientDisplayMode {
        let Some(remote_id) = self.remote_id.as_ref() else {
            return MessageRecipientDisplayMode::Sender;
        };
        if *remote_id == LabelId::drafts()
            || *remote_id == LabelId::sent()
            || *remote_id == LabelId::all_drafts()
            || *remote_id == LabelId::all_sent()
            || *remote_id == LabelId::all_scheduled()
            || *remote_id == LabelId::outbox()
        {
            MessageRecipientDisplayMode::Recipients
        } else {
            MessageRecipientDisplayMode::Sender
        }
    }
}
