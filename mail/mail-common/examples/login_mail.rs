use clap::{Args, Parser, Subcommand};
use futures::TryFutureExt;
use proton_api_core::login::Flow;
use proton_api_core::services::proton::muon::client::flow::LoginExtraInfo;
use proton_api_core::services::proton::muon::util::BoxErrExt;
use proton_api_core::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{KeyChain, KeyChainEntryKind, KeyChainError};
use proton_core_common::CoreAccountState;
use proton_mail_common::{MailContext, MailUserContext};
use secrecy::{ExposeSecret, SecretString};
use std::fs;
use std::io::{stdin, stdout, Result as IoResult, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().pretty().init();

    Cli::parse().run().inspect_err(|e| error!("{e}")).await?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Cli {
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
    async fn run(self) -> Result<()> {
        info!(?self, "starting");

        let dir = Path::new("/tmp");
        let kch = OnDiskKeyChain::new(dir)?;
        let cfg = build_config(self.app, self.env)?;
        let ctx = new_mail_ctx(dir, kch, cfg).await?;

        self.cmd.run(ctx).await
    }
}

#[derive(Debug, Subcommand)]
enum Cmd {
    Login(LoginCmd),
}

impl Cmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::Login(cmd) => cmd.run(ctx).await,
        }
    }
}

#[derive(Debug, Args)]
struct LoginCmd {
    username: String,
}

impl LoginCmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let mut flow = new_login_flow(&ctx, &self.username).await?;

        if flow.is_logged_out() {
            let pass = read("password")?;
            let info = LoginExtraInfo::default();

            flow.login(self.username, pass, info).await?;
        }

        if flow.is_awaiting_2fa() {
            flow.submit_totp(read("2nd factor")?).await?;
        }

        if flow.is_awaiting_mailbox_password() {
            flow.submit_mailbox_password(read("2nd password")?).await?;
        }

        let user_ctx = ctx
            .user_context_from_login_flow(&mut flow)
            .inspect_err(|err| error!("failed to create user context: {err:?}"))
            .await?;

        MailUserContext::initialize_async(Arc::clone(&user_ctx), &InitCb)
            .inspect_err(|(stage, err)| error!("user init failed at stage {stage:?}: {err:?}"))
            .map_err(|(_, err)| err)
            .await?;

        Ok(())
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

async fn new_mail_ctx<T: KeyChain + 'static>(
    dir: &Path,
    kch: T,
    cfg: Config,
) -> Result<Arc<MailContext>> {
    let kch = Arc::new(kch);
    let cache_path = dir.join("cache");
    let cache_size = 1 << 20;

    Ok(MailContext::new(
        dir.join("session"),
        dir.join("user"),
        cache_path.join("core"),
        cache_path.join("mail"),
        cache_size,
        None,
        kch,
        cfg,
        "",
    )
    .await?)
}

async fn new_login_flow(ctx: &MailContext, username: &str) -> Result<Flow> {
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

        return Ok(ctx.resume_login_flow(acc.remote_id, session, None).await?);
    }

    Ok(ctx.new_login_flow(None)?)
}

#[allow(clippy::print_stdout)]
fn read(prompt: &str) -> IoResult<String> {
    print!("{prompt}: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    Ok(input.trim().to_owned())
}

struct OnDiskKeyChain {
    path: PathBuf,
}

impl OnDiskKeyChain {
    fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().join("kch");

        if fs::exists(&path)? {
            info!("reusing existing keychain directory: {}", path.display());
        } else {
            fs::write(&path, SessionEncryptionKey::random().to_base64())?;
        };

        Ok(Self { path })
    }
}

impl KeyChain for OnDiskKeyChain {
    fn store_entry(&self, _: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        fs::write(&self.path, key.expose_secret().as_bytes()).box_map_err(KeyChainError::new)?;

        Ok(())
    }

    fn delete_entry(&self, _: KeyChainEntryKind) -> Result<(), KeyChainError> {
        fs::remove_file(&self.path).box_map_err(KeyChainError::new)?;

        Ok(())
    }

    fn load_entry(&self, _: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        let Ok(true) = fs::exists(&self.path) else {
            return Ok(None);
        };

        let entry = fs::read_to_string(&self.path)
            .map(SecretString::new)
            .box_map_err(KeyChainError::new)?;

        Ok(Some(entry))
    }
}

struct InitCb;

const _: () = {
    use proton_mail_common::{
        MailContextError, MailUserContextInitializationCallback, MailUserContextLoadingStage,
    };

    impl MailUserContextInitializationCallback for InitCb {
        fn on_stage(&self, stage: MailUserContextLoadingStage) {
            info!("reached user init stage: {stage:?}");
        }

        fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError) {
            info!("error at user init stage {stage:?}: {err}");
        }
    }
};
