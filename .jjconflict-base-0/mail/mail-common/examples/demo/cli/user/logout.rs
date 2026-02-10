use crate::cli::ctx::MailContextExt;
use anyhow::Result;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Logout from an account.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        ctx.get_user_ctx(&self.username).await?.logout().await?;

        Ok(())
    }
}
