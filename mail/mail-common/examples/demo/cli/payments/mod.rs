use crate::cli::ctx::MailContextExt;
use anyhow::Result;
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;

mod resources;
mod subscription;

/// Manage payments.
#[derive(Debug, Args)]
pub struct Cmd {
    #[arg(long)]
    username: String,

    #[command(subcommand)]
    cmd: PaymentsSubCmd,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let ctx = ctx.get_user_ctx(&self.username).await?;

        self.cmd.run(ctx).await
    }
}

#[derive(Debug, Subcommand)]
enum PaymentsSubCmd {
    #[command(subcommand)]
    Resources(resources::Cmd),
    Subscription(subscription::Cmd),
}

impl PaymentsSubCmd {
    async fn run(self, ctx: Arc<MailUserContext>) -> Result<()> {
        match self {
            Self::Resources(cmd) => cmd.run(ctx).await,
            Self::Subscription(cmd) => cmd.run(ctx).await,
        }
    }
}
