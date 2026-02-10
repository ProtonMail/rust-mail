use crate::app::events::{Proxy, UserEvent};
use crate::cli::cfg::new_api_config;
use crate::cli::ctx::new_mail_ctx;
use crate::keychain::OnDiskKeyChain;
use crate::notifier::HvNotifier;
use anyhow::Result;
use app::new_app_details;
use clap::Parser;
use proton_mail_common::MailContext;
use std::io::{Result as IoResult, Write, stdin, stdout};
use std::path::PathBuf;
use std::sync::Arc;

mod app;
mod cfg;
mod ctx;
mod payments;
mod user;

const APP_NAME: &str = "proton-mail-common-demo";

#[derive(Debug, Parser)]
pub struct Cli {
    /// The app platform to use. Used to identify the app in API.
    #[arg(long, default_value = "ios")]
    app_platform: String,

    /// The app product to use. Used to identify the app in API.
    #[arg(long, default_value = "mail")]
    app_product: String,

    /// The app version to use. Used to identify the app in API.
    #[arg(long, default_value = "7.1.0")]
    app_version: String,

    /// The environment to connect to.
    #[arg(long)]
    env: Option<String>,

    #[arg(long)]
    device: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

impl Cli {
    pub async fn run(proxy: impl Proxy + 'static) -> Result<()> {
        Self::parse().run_cmd(proxy.clone()).await?;

        proxy.send_event(UserEvent::Exit)?;

        Ok(())
    }

    async fn run_cmd(self, proxy: impl Proxy + 'static) -> Result<()> {
        let dir = tempdir(self.device).inspect(|dir| info!("{}", dir.display()))?;
        let kch = Arc::new(OnDiskKeyChain::new(&dir)?);
        let hvn = Arc::new(HvNotifier::new(proxy));
        let app = new_app_details(self.app_platform, self.app_product, self.app_version);
        let cfg = new_api_config(app, self.env)?;
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

fn read(prompt: &str) -> IoResult<String> {
    print!("{prompt}: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    Ok(input.trim().to_owned())
}

fn tempdir(device: Option<String>) -> Result<PathBuf> {
    let mut dir = std::env::temp_dir().join(APP_NAME);

    if let Some(device_dir) = device {
        dir = dir.join(device_dir);
    }

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}
