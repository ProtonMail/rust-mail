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
        let mut flow = ctx.new_or_resume_login_flow(Some(username)).await?;

        loop {
            if flow.is_logged_out() {
                let _ = flow
                    .login_with_credentials(username, read("password")?, None)
                    .inspect_err(|e| warn!("{e}"))
                    .await;
            } else if flow.is_awaiting_2fa() {
                let _ = flow
                    .submit_totp(read("2nd factor")?)
                    .inspect_err(|e| warn!("{e}"))
                    .await;
            } else if flow.is_awaiting_new_password() {
                let _ = flow
                    .submit_new_password(read("new password")?)
                    .inspect_err(|e| warn!("{e}"))
                    .await;
            } else if flow.is_awaiting_mailbox_password() {
                let _ = flow
                    .submit_mailbox_password(read("2nd password")?)
                    .inspect_err(|e| warn!("{e}"))
                    .await;
            } else if flow.is_logged_in() {
                let user_ctx = ctx
                    .user_context_from_login_flow(&mut flow)
                    .inspect_err(|err| error!("failed to create user context: {err:?}"))
                    .await?;

                return Ok((flow, user_ctx));
            }
        }
    }
}
