use crate::Result;
use proton_mail_common::MailContext;
use std::sync::Arc;

mod list;
mod login;
mod logout;
mod signup;
mod switch;

/// Manage users.
#[derive(Debug, Args)]
pub struct Cmd {
    #[command(subcommand)]
    cmd: UserSubCmd,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        self.cmd.run(ctx).await
    }
}

#[derive(Debug, Subcommand)]
enum UserSubCmd {
    List(list::Cmd),
    Login(login::Cmd),
    Signup(signup::Cmd),
    Switch(switch::Cmd),
    Logout(logout::Cmd),
}

impl UserSubCmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::List(cmd) => cmd.run(ctx).await,
            Self::Login(cmd) => cmd.run(ctx).await,
            Self::Signup(cmd) => cmd.run(ctx).await,
            Self::Switch(cmd) => cmd.run(ctx).await,
            Self::Logout(cmd) => cmd.run(ctx).await,
        }
    }
}
