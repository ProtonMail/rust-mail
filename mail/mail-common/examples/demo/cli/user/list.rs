use anyhow::Result;
use proton_core_common::CoreAccountState;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// List available accounts.
#[derive(Debug, Args)]
pub struct Cmd {
    // ...
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let primary = ctx.get_primary_account().await?;

        for acount in ctx.get_accounts().await? {
            let Some(state) = ctx.get_account_state(acount.remote_id.clone()).await? else {
                continue;
            };

            if (primary.as_ref()).is_some_and(|p| p.remote_id == acount.remote_id) {
                print!("* {}", acount.name_or_addr);
            } else {
                print!("  {}", acount.name_or_addr);
            }

            match state {
                CoreAccountState::LoggedIn(_) => println!(),
                CoreAccountState::NeedMbp(_) => println!(" (MBP)"),
                CoreAccountState::NeedTfa(_) => println!(" (2FA)"),
                CoreAccountState::LoggedOut => println!(" (OUT)"),
                CoreAccountState::NotReady => println!(" (BAD)"),
            }
        }

        Ok(())
    }
}
