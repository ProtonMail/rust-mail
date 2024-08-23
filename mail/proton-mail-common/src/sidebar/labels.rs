use crate::actions::labels::Expand;
use crate::{AppError, MailContextError};
use proton_core_common::datatypes::{LabelId, LocalId};
use stash::orm::Model;
use stash::params;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tracing::error;

use crate::datatypes::labels::custom_folder::CustomFolder;
use crate::datatypes::labels::custom_labels::CustomLabel;
use crate::datatypes::labels::system_labels::SystemLabel;
use crate::datatypes::{AlmostAllMail, ShowMoved};
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
    pub async fn system_labels(&self) -> SidebarResult<Vec<SystemLabel>> {
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
        Ok(SystemLabel::from_labels(labels.as_slice(), interface).await?)
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// Use `None` to get the root `Folders`
    /// Use the id of a `Folders` to get its children
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_folders(&self) -> SidebarResult<Vec<CustomFolder>> {
        let labels: Vec<_> = self.all_custom_folders().await?;

        // Create Hierarchy
        let mut index: HashMap<_, _> = labels
            .iter()
            .map(|l| {
                (
                    l.local_id.as_u64(),
                    Rc::new(RefCell::new(Hierarchy {
                        id: l.local_id.as_u64(),
                        parent_id: l.parent_id.map(|i| i.as_u64()),
                        children: vec![],
                    })),
                )
            })
            .collect();

        // Construct Hierarchy
        for value in &labels {
            if let Some(parent_id) = value.parent_id {
                let rc = index.get(&value.local_id.as_u64()).unwrap().clone();
                index
                    .entry(parent_id.as_u64())
                    .and_modify(|f| f.borrow_mut().children.push(rc));
            }
        }

        // Index CustomFolder by their local_id
        let mut by_id = labels.into_iter().fold(HashMap::new(), |mut acc, f| {
            acc.insert(f.local_id.as_u64(), f);
            acc
        });

        // Map CustomFolders to their hierarchy
        let mut result: Vec<_> = index
            .iter()
            // Keep only root Folders
            .filter(|(_, f)| f.borrow().parent_id.is_none())
            .map(|(_, f)| {
                let mut folder = by_id.remove(&f.borrow().id).unwrap();
                folder.children = f.borrow().map_children(&mut by_id);
                folder
            })
            .collect();
        result.sort_by_cached_key(|a| a.display_order);

        Ok(result)
    }

    /// Get all the [`CustomFolder`].
    pub async fn all_custom_folders(&self) -> SidebarResult<Vec<CustomFolder>> {
        let interface = self.user_ctx.user_stash();
        let labels = Label::find(
            "WHERE label_type = ? ORDER BY display_order",
            params![LabelType::Folder],
            interface,
            None,
        )
        .await?;
        Ok(CustomFolder::from_labels(labels.as_slice(), interface).await?)
    }

    /// Get the list of Custom Labels to display in the sidebar.
    ///
    /// # Errors
    ///   * Database request fail
    ///
    pub async fn custom_labels(&self) -> SidebarResult<Vec<CustomLabel>> {
        let interface = self.user_ctx.user_stash();
        let labels = Label::find(
            "WHERE label_type = ? ORDER BY display_order",
            params![LabelType::Label],
            interface,
            None,
        )
        .await?;
        Ok(CustomLabel::from_labels(labels.as_slice(), interface).await?)
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

#[derive(Debug)]
struct Hierarchy {
    id: u64,
    parent_id: Option<u64>,
    children: Vec<Rc<RefCell<Hierarchy>>>,
}

impl Hierarchy {
    // Map the children of the current node to a vec of CustomFolder
    // Called recursively on all children
    // Note: Sort the children using display_order
    fn map_children(&self, index: &mut HashMap<u64, CustomFolder>) -> Vec<CustomFolder> {
        if self.children.is_empty() {
            vec![]
        } else {
            let mut result: Vec<_> = self
                .children
                .iter()
                .map(|c| {
                    let mut folder = index.remove(&c.borrow().id).unwrap();
                    folder.children = c.borrow().map_children(index);
                    folder
                })
                .collect();
            result.sort_by_cached_key(|a| a.display_order);
            result
        }
    }
}
