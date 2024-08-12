use std::sync::Arc;

use stash::stash::StashError;

use crate::{MailContextError, MailUserContext};

pub mod labels;

#[derive(Debug, thiserror::Error)]
pub enum SidebarError {
    #[error("Couldn't load Settings from database")]
    SettingsNotFound,
    #[error("Mail Context Error: {0}")]
    MailContext(#[from] MailContextError),
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
