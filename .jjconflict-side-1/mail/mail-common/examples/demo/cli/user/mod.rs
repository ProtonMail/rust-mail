use anyhow::Result;
use proton_mail_common::MailContext;
use std::sync::Arc;

mod addresses;
mod list;
mod login;
mod logout;
mod password;
mod qr_login;
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
    Addresses(addresses::Cmd),
    List(list::Cmd),
    Login(login::Cmd),
    Signup(signup::Cmd),
    Switch(switch::Cmd),
    Logout(logout::Cmd),
    Password(password::Cmd),
    QrTarget(qr_login::TargetCmd),
    QrHost(qr_login::HostCmd),
}

impl UserSubCmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::Addresses(cmd) => cmd.run(ctx).await,
            Self::List(cmd) => cmd.run(ctx).await,
            Self::Login(cmd) => cmd.run(ctx).await,
            Self::Signup(cmd) => cmd.run(ctx).await,
            Self::Switch(cmd) => cmd.run(ctx).await,
            Self::Logout(cmd) => cmd.run(ctx).await,
            Self::Password(cmd) => cmd.run(ctx).await,
            Self::QrTarget(cmd) => cmd.run(ctx).await,
            Self::QrHost(cmd) => cmd.run(ctx).await,
        }
    }
}
