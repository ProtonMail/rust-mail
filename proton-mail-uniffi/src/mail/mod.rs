mod logging;
mod login;
mod mailbox;
mod session;
mod type_forwarding;
mod user_session;

pub use login::*;
pub use mailbox::*;
use proton_mail_common::exports::anyhow::anyhow;
pub use session::*;
pub use user_session::*;

#[inline]
fn map_task_join_error(e: Box<dyn std::error::Error>) -> MailSessionError {
    MailSessionError::Other(anyhow!("Failed to join task: {e}"))
}
