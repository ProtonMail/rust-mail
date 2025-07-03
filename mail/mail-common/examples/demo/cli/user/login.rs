use crate::cli::ctx::MailContextExt;
use crate::cli::read;
use anyhow::Result;
use futures::TryFutureExt;
use proton_account_api::login::LoginFlow;
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;

/// Login to an account.
#[derive(Debug, Args)]
pub struct Cmd {
    pub username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let _ = Self::login(ctx, &self.username).await?;
        Ok(())
    }

    pub async fn login(
        ctx: Arc<MailContext>,
        username: &str,
    ) -> Result<(LoginFlow, Arc<MailUserContext>)> {
        let mut flow = ctx.new_login_flow(Some(username)).await?;

        if flow.is_logged_out() {
            flow.login_with_credentials(username.to_owned(), read("password")?, None)
                .await?;
        }

        if flow.is_awaiting_2fa() {
            flow.submit_totp(read("2nd factor")?).await?;
        }

        if flow.is_awaiting_mailbox_password() {
            flow.submit_mailbox_password(read("2nd password")?).await?;
        }

        let user_ctx = ctx
            .user_context_from_login_flow(&mut flow)
            .inspect_err(|err| error!("failed to create user context: {err:?}"))
            .await?;

        Ok((flow, user_ctx))
    }
}
