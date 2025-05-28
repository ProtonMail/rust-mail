use crate::Result;
use crate::cli::get_user_ctx;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Logout from an account.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        get_user_ctx(&ctx, &self.username).await?.logout().await?;

        Ok(())
    }
}
