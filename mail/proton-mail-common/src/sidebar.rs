use proton_core_common::datatypes::LabelId;
use std::sync::Arc;

use stash::stash::StashError;

use crate::{MailUserContext, MailboxError};

pub mod labels;

#[derive(Debug, thiserror::Error)]
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

pub type SidebarResult<T> = Result<T, SidebarError>;

/// Represents the sidebar where user can navigate between mailbox, folders, labels, settings, ...
pub struct Sidebar {
    pub user_ctx: Arc<MailUserContext>,
}

impl Sidebar {
    pub fn new(user_ctx: Arc<MailUserContext>) -> Self {
        Self { user_ctx }
    }
}
