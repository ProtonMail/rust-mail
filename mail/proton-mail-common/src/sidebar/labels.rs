use crate::actions::labels::Expand;
use crate::{AppError, MailContextError};
use proton_core_common::datatypes::{LabelId, LocalId};
use stash::orm::Model;
use stash::params;
use tracing::error;

use crate::datatypes::{AlmostAllMail, ContextualLabel, ShowMoved};
use crate::datatypes::{LabelType, SystemLabelId};
use crate::models::{Label, MailSettings, MAIL_SETTINGS_ID};
use crate::sidebar::{Sidebar, SidebarError, SidebarResult};

impl Sidebar {
    /// Get the list of the System Folder to display in the sidebar.
    ///
    /// That list is filtered in function of [`MailSettings::almost_all_mail`],
    /// [`MailSettings::show_moved`] and some are hidden when empty (`Scheduled`, `Outbox` and
    /// `Snoozed`)
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn system_labels(&self) -> SidebarResult<Vec<ContextualLabel>> {
        let interface = self.user_ctx.user_stash();
        let settings = MailSettings::load(MAIL_SETTINGS_ID.into(), interface)
            .await?
            .unwrap_or_default();

        let mut labels = vec![self.get_label(LabelId::inbox()).await?];
        if settings.show_moved == ShowMoved::KeepInDrafts
            || settings.show_moved == ShowMoved::KeepBoth
        {
            labels.push(self.get_label(LabelId::all_drafts()).await?);
        } else {
            labels.push(self.get_label(LabelId::drafts()).await?);
        }
        let all_scheduled = self.get_label(LabelId::all_scheduled()).await?;
        if all_scheduled.total_msg != 0 || all_scheduled.total_conv != 0 {
            labels.push(all_scheduled);
        }
        let outbox = self.get_label(LabelId::outbox()).await?;
        if outbox.total_conv != 0 || outbox.total_msg != 0 {
            labels.push(outbox);
        }
        let snoozed = self.get_label(LabelId::snoozed()).await?;
        if snoozed.total_conv != 0 || snoozed.total_msg != 0 {
            labels.push(snoozed);
        }
        labels.push(self.get_label(LabelId::starred()).await?);
        if settings.show_moved == ShowMoved::KeepInSent
            || settings.show_moved == ShowMoved::KeepBoth
        {
            labels.push(self.get_label(LabelId::all_sent()).await?);
        } else {
            labels.push(self.get_label(LabelId::sent()).await?);
        }
        labels.push(self.get_label(LabelId::spam()).await?);
        labels.push(self.get_label(LabelId::archive()).await?);
        labels.push(self.get_label(LabelId::trash()).await?);
        if settings.almost_all_mail == AlmostAllMail::AllMail {
            labels.push(self.get_label(LabelId::all_mail()).await?);
        } else {
            labels.push(self.get_label(LabelId::almost_all_mail()).await?);
        }
        Ok(ContextualLabel::from_labels(labels.as_slice(), interface).await?)
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// Use `None` to get the root `Folders`
    /// Use the id of a `Folders` to get its children
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_folders(
        &self,
        parent_id: Option<LocalId>,
    ) -> SidebarResult<Vec<ContextualLabel>> {
        let interface = self.user_ctx.user_stash();
        let labels = if let Some(parent_id) = parent_id {
            Label::find(
                "WHERE label_type = ? AND local_parent_id = ? ORDER BY display_order",
                params![LabelType::Folder, parent_id],
                interface,
                None,
            )
            .await?
        } else {
            Label::find(
                "WHERE label_type = ? AND local_parent_id is NULL ORDER BY display_order",
                params![LabelType::Folder],
                interface,
                None,
            )
            .await?
        };
        Ok(ContextualLabel::from_labels(labels.as_slice(), interface).await?)
    }

    /// Get the list of Custom Labels to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_labels(&self) -> SidebarResult<Vec<ContextualLabel>> {
        let interface = self.user_ctx.user_stash();
        let labels = Label::find(
            "WHERE label_type = ? ORDER BY display_order",
            params![LabelType::Label],
            interface,
            None,
        )
        .await?;
        Ok(ContextualLabel::from_labels(labels.as_slice(), interface).await?)
    }

    /// Set folder `expanded` field to it's collapsed state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn collapse_folder(&self, local_id: LocalId) -> SidebarResult<()> {
        self.user_ctx
            .execute_action(Expand::collapse(local_id))
            .await?;
        Ok(())
    }

    /// Set folder `expanded` field to it's expanded state
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn expand_folder(&self, local_id: LocalId) -> SidebarResult<()> {
        self.user_ctx
            .execute_action(Expand::expand(local_id))
            .await?;
        Ok(())
    }

    /// Get a [`Label`] given a [`LabelId`]
    async fn get_label(&self, label_id: LabelId) -> SidebarResult<Label> {
        Label::find_first(
            "WHERE remote_id = ?",
            params![label_id.clone()],
            self.user_ctx.user_stash(),
        )
        .await?
        .ok_or_else(|| {
            error!("System Label don't exist: {}", label_id);
            SidebarError::MailContext(MailContextError::App(AppError::RemoteLabelDoesNotExist(
                label_id,
            )))
        })
    }
}
