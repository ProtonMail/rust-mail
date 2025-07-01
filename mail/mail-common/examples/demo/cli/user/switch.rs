use crate::cli::ctx::MailContextExt;
use anyhow::Result;
use futures::TryFutureExt;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Switch the active account.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let user_id = ctx
            .get_user_ctx(&self.username)
            .map_ok(|ctx| ctx.user_id().to_owned())
            .await?;

        ctx.set_primary_account(user_id).await?;

        Ok(())
    }
}
