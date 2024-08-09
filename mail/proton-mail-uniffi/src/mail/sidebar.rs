use crate::core::datatypes::LabelId;
use crate::mail::datatypes::Label;
use crate::mail::MailUserSession;
use crate::mail::MailboxError;
use stash::stash::StashError;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SidebarError {
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Couldn't load Settings from database")]
    SettingsNotFound,
    #[error("Mailbox Error: {0}")]
    Mailbox(#[from] MailboxError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
}

type SidebarResult<T> = Result<T, SidebarError>;

impl From<proton_mail_common::SidebarError> for SidebarError {
    fn from(error: proton_mail_common::SidebarError) -> Self {
        match error {
            proton_mail_common::SidebarError::RemoteLabelNotFound(label_id) => {
                Self::RemoteLabelNotFound(label_id.into())
            }
            proton_mail_common::SidebarError::SettingsNotFound => Self::SettingsNotFound,
            proton_mail_common::SidebarError::Mailbox(e) => Self::Mailbox(e.into()),
            proton_mail_common::SidebarError::Stash(e) => Self::Stash(e),
        }
    }
}

/// A [`Sidebar`] provides a gateway to manipulating actions accessible from sidebar
#[derive(uniffi::Object)]
pub struct Sidebar {
    /// The inner sidebar, which is the real internal type.
    sidebar: proton_mail_common::Sidebar,
}

#[uniffi::export]
impl Sidebar {
    #[must_use]
    #[uniffi::constructor]
    pub fn new(ctx: &MailUserSession) -> Self {
        Self {
            sidebar: proton_mail_common::Sidebar::new(ctx.ctx().clone()),
        }
    }

    /// Get the list of the System Folder to display in the sidebar.
    ///
    /// That list is filtered in function of [`MailSettings::almost_all_mail`] and some are hidden
    /// when empty (`Scheduled`, `Outbox` and `Snoozed`)
    pub async fn system_labels(&self) -> SidebarResult<Vec<Label>> {
        Ok(self
            .sidebar
            .system_labels()
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Get the list of Custom Folders to display in the sidebar.
    ///
    /// Use `None` to get the root `Folders`
    /// Use the id of a `Folders` to get its children
    pub async fn custom_folders(&self, parent_id: Option<u64>) -> SidebarResult<Vec<Label>> {
        // TODO: how to expose a callback ?
        Ok(self
            .sidebar
            .custom_folders(parent_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Get the list of Custom Labels to display in the sidebar.
    pub async fn custom_labels(&self) -> SidebarResult<Vec<Label>> {
        // TODO: how to expose a callback ?
        Ok(self
            .sidebar
            .custom_labels()
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Set folder `expanded` field to it's collapsed state
    pub async fn collapse_folder(&self, local_id: u64) -> SidebarResult<()> {
        Ok(self.sidebar.collapse_folder(local_id).await?)
    }

    /// Set folder `expanded` field to it's expanded state
    pub async fn expand_folder(&self, local_id: u64) -> SidebarResult<()> {
        Ok(self.sidebar.expand_folder(local_id).await?)
    }
}
