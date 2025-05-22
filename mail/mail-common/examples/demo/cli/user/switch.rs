use crate::Result;
use crate::cli::get_user_ctx;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Switch the active account.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let user_ctx = get_user_ctx(&ctx, &self.username).await?;

        ctx.set_primary_account(user_ctx.user_id().to_owned())
            .await?;

        Ok(())
    }
}
