mod context;
mod logging;
mod login;
mod mailbox;
mod type_forwarding;
mod user_context;

pub use context::*;
pub use login::*;
use proton_mail_common::exports::anyhow::anyhow;
pub use user_context::*;

#[inline]
fn map_task_join_error(e: Box<dyn std::error::Error>) -> MailContextError {
    MailContextError::Other(anyhow!("Failed to join task: {e}"))
}
