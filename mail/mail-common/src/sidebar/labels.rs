use crate::actions::labels::Expand;
use crate::datatypes::labels::hierarchy::custom_folder_hierarchy;
use crate::{AppError, MailContextError, MailUserContext};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelType, LocalLabelId};
use proton_core_common::models::Label;
use stash::params;
use stash::{orm::Model, stash::Tether};
use tracing::error;

use crate::datatypes::ShowMoved;
use crate::datatypes::SystemLabelId;
use crate::datatypes::labels::custom_folder::CustomFolder;
use crate::datatypes::labels::custom_labels::CustomLabel;
use crate::datatypes::labels::system_labels::SystemLabel;
use crate::models::{LabelWithCounters, MailSettings};
use crate::sidebar::{Sidebar, SidebarError, SidebarResult};

impl Sidebar {
    pub async fn system_labels(&self, tether: &Tether) -> SidebarResult<Vec<SystemLabel>> {
        let settings = MailSettings::get_or_default(tether).await;
        let mut labels = vec![self.get_label(tether, LabelId::inbox()).await?];

        if settings.show_moved == ShowMoved::KeepInDrafts
            || settings.show_moved == ShowMoved::KeepBoth
        {
            labels.push(self.get_label(tether, LabelId::all_drafts()).await?);
        } else {
            labels.push(self.get_label(tether, LabelId::drafts()).await?);
        }

        let all_scheduled = self
            .get_label_with_counters(tether, LabelId::all_scheduled())
            .await?;

        if all_scheduled.total_msg != 0 || all_scheduled.total_conv != 0 {
            labels.push(all_scheduled.label);
        }

        let outbox = self
            .get_label_with_counters(tether, LabelId::outbox())
            .await?;

        if outbox.total_conv != 0 || outbox.total_msg != 0 {
            labels.push(outbox.label);
        }

        let snoozed = self
            .get_label_with_counters(tether, LabelId::snoozed())
            .await?;

        if snoozed.total_conv != 0 || snoozed.total_msg != 0 {
            labels.push(snoozed.label);
        }

        labels.push(self.get_label(tether, LabelId::starred()).await?);

        if settings.show_moved == ShowMoved::KeepInSent
            || settings.show_moved == ShowMoved::KeepBoth
        {
            labels.push(self.get_label(tether, LabelId::all_sent()).await?);
        } else {
            labels.push(self.get_label(tether, LabelId::sent()).await?);
        }

        labels.push(self.get_label(tether, LabelId::spam()).await?);
        labels.push(self.get_label(tether, LabelId::archive()).await?);
        labels.push(self.get_label(tether, LabelId::trash()).await?);
        labels.push(self.get_label(tether, settings.all_mail()).await?);

        Ok(SystemLabel::from_labels(labels.as_slice(), tether).await?)
    }

    pub async fn custom_folders(&self, tether: &Tether) -> SidebarResult<Vec<CustomFolder>> {
        let labels = self.all_custom_folders(tether).await?;

        Ok(custom_folder_hierarchy(&labels))
    }

    pub async fn all_custom_folders(&self, tether: &Tether) -> SidebarResult<Vec<CustomFolder>> {
        let labels = Label::find_by_kind(LabelType::Folder, tether).await?;

        Ok(CustomFolder::from_labels(labels.as_slice(), tether).await?)
    }

    pub async fn custom_labels(&self, tether: &Tether) -> SidebarResult<Vec<CustomLabel>> {
        let labels = Label::find_by_kind(LabelType::Label, tether).await?;

        Ok(CustomLabel::from_labels(labels.as_slice(), tether).await?)
    }

    pub async fn collapse_folder(
        &self,
        ctx: &MailUserContext,
        local_id: LocalLabelId,
    ) -> SidebarResult<()> {
        ctx.queue_action(Expand::collapse(local_id)).await?;

        Ok(())
    }

    pub async fn expand_folder(
        &self,
        ctx: &MailUserContext,
        local_id: LocalLabelId,
    ) -> SidebarResult<()> {
        ctx.queue_action(Expand::expand(local_id)).await?;

        Ok(())
    }

    async fn get_label(&self, tether: &Tether, label_id: LabelId) -> SidebarResult<Label> {
        Label::find_first("WHERE remote_id = ?", params![label_id.clone()], tether)
            .await?
            .ok_or_else(|| {
                error!("System Label don't exist: {:?}", label_id);
                SidebarError::MailContext(MailContextError::App(AppError::RemoteLabelDoesNotExist(
                    label_id,
                )))
            })
    }

    async fn get_label_with_counters(
        &self,
        tether: &Tether,
        label_id: LabelId,
    ) -> SidebarResult<LabelWithCounters> {
        LabelWithCounters::find_first("WHERE remote_id = ?", params![label_id.clone()], tether)
            .await?
            .ok_or_else(|| {
                error!("System Label don't exist: {:?}", label_id);
                SidebarError::MailContext(MailContextError::App(AppError::RemoteLabelDoesNotExist(
                    label_id,
                )))
            })
    }
}
