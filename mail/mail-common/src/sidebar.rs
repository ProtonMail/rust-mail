use stash::stash::StashError;

use crate::{AppError, MailContextError};

pub mod labels;

#[derive(Debug, thiserror::Error)]
pub enum SidebarError {
    #[error("App Error: {0}")]
    AppError(#[from] AppError),
    #[error("Mail Context Error: {0}")]
    MailContext(#[from] MailContextError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
}

pub type SidebarResult<T> = Result<T, SidebarError>;

/// Represents the sidebar where user can navigate between mailbox, folders, labels, settings, ...
#[derive(Clone)]
pub struct Sidebar;
