use crate::Result;
use crate::cli::read;
use futures::TryFutureExt;
use proton_account_api::login::LoginFlow;
use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_common::CoreAccountState;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Login to an account.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let mut flow = new_login_flow(&ctx, &self.username).await?;

        if flow.is_logged_out() {
            let pass = read("password")?;
            let info = LoginExtraInfo::default();

            flow.login(self.username.clone(), pass, info).await?;
        }

        if flow.is_awaiting_2fa() {
            flow.submit_totp(read("2nd factor")?).await?;
        }

        if flow.is_awaiting_mailbox_password() {
            flow.submit_mailbox_password(read("2nd password")?).await?;
        }

        _ = ctx
            .user_context_from_login_flow(&mut flow)
            .inspect_err(|err| error!("failed to create user context: {err:?}"))
            .await?;

        Ok(())
    }
}

async fn new_login_flow(ctx: &MailContext, username: &str) -> Result<LoginFlow> {
    for acc in ctx.get_accounts().await? {
        if acc.name_or_addr != username {
            continue;
        }

        let session = match ctx.get_account_state(acc.remote_id.clone()).await? {
            Some(CoreAccountState::LoggedIn(_)) => Err("account already logged in")?,
            Some(CoreAccountState::NeedMbp(mut s)) => s.pop().unwrap(),
            Some(CoreAccountState::NeedTfa(mut s)) => s.pop().unwrap(),
            _ => continue,
        };

        return Ok(ctx.resume_login_flow(acc.remote_id, session).await?);
    }

    Ok(ctx.new_login_flow().await?)
}
