use crate::Result;
use crate::app::UserEvent;
use crate::keychain::OnDiskKeyChain;
use crate::notifier::HvNotifier;
use clap::Parser;
use proton_core_api::session::Config;
use proton_core_api::verification::ChallengeNotifier;
use proton_core_common::CoreAccountState;
use proton_core_common::os::KeyChain;
use proton_mail_common::context::{EventPollMode, ShouldInitializeMailUserContext};
use proton_mail_common::{MailContext, MailUserContext};
use std::io::{Result as IoResult, Write, stdin, stdout};
use std::path::Path;
use std::sync::Arc;
use tao::event_loop::EventLoopProxy;

mod payments;
mod user;

#[derive(Debug, Parser)]
pub struct Cli {
    /// The app version to use.
    #[arg(long)]
    app: Option<String>,

    /// The environment to connect to.
    #[arg(long)]
    env: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

impl Cli {
    pub async fn run(proxy: EventLoopProxy<UserEvent>) -> Result<()> {
        Self::parse().run_cmd(proxy.clone()).await?;

        proxy.send_event(UserEvent::Exit)?;

        Ok(())
    }

    async fn run_cmd(self, proxy: EventLoopProxy<UserEvent>) -> Result<()> {
        let dir = std::env::temp_dir();
        let kch = Arc::new(OnDiskKeyChain::new(&dir)?);
        let hvn = Arc::new(HvNotifier::new(proxy));
        let cfg = build_config(self.app, self.env)?;
        let ctx = new_mail_ctx(&dir, cfg, kch, hvn).await?;

        self.cmd.run(ctx).await
    }
}

#[derive(Debug, Subcommand)]
enum Cmd {
    User(user::Cmd),
    Payments(payments::Cmd),
}

impl Cmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::User(cmd) => cmd.run(ctx).await,
            Self::Payments(cmd) => cmd.run(ctx).await,
        }
    }
}

fn build_config(app: Option<String>, env: Option<String>) -> Result<Config> {
    let mut cfg = Config::default();

    if let Some(app) = app {
        cfg.app_version = app;
    }

    if let Some(env) = env {
        cfg.env_id = env.parse()?;
    }

    Ok(cfg)
}

async fn new_mail_ctx<K, N>(
    dir: &Path,
    cfg: Config,
    kch: Arc<K>,
    hvn: Arc<N>,
) -> Result<Arc<MailContext>>
where
    K: KeyChain + 'static,
    N: ChallengeNotifier + 'static,
{
    const CACHE_SIZE: u64 = 1 << 20;

    Ok(MailContext::new(
        dir.join("session"),
        dir.join("user"),
        dir.join("cache").join("core"),
        dir.join("cache").join("mail"),
        CACHE_SIZE,
        None,
        kch,
        cfg,
        Some(hvn),
        None,
        None,
        EventPollMode::Manual,
    )
    .await?)
}

async fn get_user_ctx(ctx: &Arc<MailContext>, username: &str) -> Result<Arc<MailUserContext>> {
    for acc in ctx.get_accounts().await? {
        if acc.name_or_addr != username {
            continue;
        }

        let Some(CoreAccountState::LoggedIn(mut s)) =
            ctx.get_account_state(acc.remote_id.clone()).await?
        else {
            continue;
        };

        let Some(session) = ctx.get_session(s.pop().unwrap()).await? else {
            continue;
        };

        return Ok(ctx
            .user_context_from_session(&session, None, ShouldInitializeMailUserContext::Yes)
            .await?);
    }

    Err("account not found")?
}

fn read(prompt: &str) -> IoResult<String> {
    print!("{prompt}: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    Ok(input.trim().to_owned())
}
