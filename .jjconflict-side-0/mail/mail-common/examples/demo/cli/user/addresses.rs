use crate::cli::ctx::MailContextExt;
use anyhow::Result;
use proton_core_common::models::Address;
use proton_mail_common::MailContext;
use std::sync::Arc;

/// List all addresses owned by the user.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let tether = ctx
            .get_user_ctx(&self.username)
            .await?
            .user_stash()
            .connection()
            .await?;

        for address in Address::all_send_enabled(&tether).await? {
            if address.display_name.is_empty() {
                println!("  - {}", address.email);
            } else {
                println!("  - {}: {}", address.display_name, address.email);
            }
        }

        Ok(())
    }
}
