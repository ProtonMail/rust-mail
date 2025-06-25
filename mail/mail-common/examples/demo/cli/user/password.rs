use crate::Result;
use crate::cli::ctx::MailContextExt;
use crate::cli::read;
use clap::Args;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Change user password.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let mut flow = ctx
            .get_user_ctx(&self.username)
            .await?
            .new_password_flow()
            .await?;

        if flow.is_awaiting_password() {
            flow.submit_password(read("current password")?).await?;
        }

        if flow.is_awaiting_2fa() {
            flow.submit_totp(read("2nd factor")?).await?;
        }

        if flow.is_awaiting_new_password() {
            flow.submit_new_password(read("new password")?).await?;
        }

        if !flow.is_complete() {
            bail!("expected completed flow");
        }

        Ok(())
    }
}
