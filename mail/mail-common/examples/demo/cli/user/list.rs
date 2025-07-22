use anyhow::Result;
use proton_core_common::{CoreAccountState, db::account::CoreAccount};
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

        for account in ctx.get_accounts().await? {
            let Some(state) = ctx.get_account_state(account.remote_id.clone()).await? else {
                continue;
            };

            if (primary.as_ref()).is_some_and(|p| p.remote_id == account.remote_id) {
                print!("* {}", get_account_name(&account));
            } else {
                print!("  {}", get_account_name(&account));
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

fn get_account_name(account: &CoreAccount) -> &str {
    if let Some(addr) = &account.primary_addr {
        return addr;
    }

    if let Some(name) = &account.username {
        return name;
    }

    &account.name_or_addr
}
