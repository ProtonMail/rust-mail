#![allow(async_fn_in_trait)]
#![allow(clippy::module_inception)]

#[cfg(test)]
#[path = "../tests/models/mailbox_labels.rs"]
mod mailbox_labels;

use crate::datatypes::{MessageRecipientDisplayMode, SystemLabelId, ViewMode};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelType, SystemLabel};
use proton_core_common::{datatypes::LocalLabelId, models::Label};
use stash::stash::Tether;
use stash::{macros::Model, stash::StashError};

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
}

impl MailboxLabels {
    /// Constructor - note: [`MailboxLabels`] does not implement [`Default`] trait
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            initialized: false,
        }
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

    fn is_movable_into_folder(&self) -> bool;

    fn is_movable_out_of_folder(&self) -> bool;

    fn recipient_display_mode(&self) -> MessageRecipientDisplayMode;

    fn is_snooze_location(&self) -> bool;
}

impl MailLabel for Label {
    async fn view_mode(&self, tether: &Tether) -> Result<ViewMode, StashError> {
        if let Some(remote_id) = self.remote_id.as_ref()
            && (*remote_id == LabelId::drafts()
                || *remote_id == LabelId::sent()
                || *remote_id == LabelId::all_drafts()
                || *remote_id == LabelId::all_sent()
                || *remote_id == LabelId::all_scheduled()
                || *remote_id == LabelId::outbox())
        {
            return Ok(ViewMode::Messages);
        }
        Ok(MailSettings::get_or_default(tether).await.view_mode)
    }

    fn is_movable_into_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self
                .remote_id
                .as_ref()
                .is_some_and(|rid| LabelId::movable_sys_folder_list().contains(rid))
    }

    fn is_movable_out_of_folder(&self) -> bool {
        let mut movable_folders = LabelId::movable_sys_folder_list().to_vec();

        movable_folders.push(LabelId::drafts());
        movable_folders.push(LabelId::sent());

        self.label_type == LabelType::Folder
            || self
                .remote_id
                .as_ref()
                .is_some_and(|rid| movable_folders.contains(rid))
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

    fn is_snooze_location(&self) -> bool {
        SystemLabel::new(self).is_some_and(|label| label.is_snooze_location())
    }
}
