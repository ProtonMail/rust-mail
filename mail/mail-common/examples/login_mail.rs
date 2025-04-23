#![allow(clippy::print_stdout)]

use async_trait::async_trait;
use clap::{Args, Parser, Subcommand};
use futures::TryFutureExt;
use proton_api_core::login::Flow;
use proton_api_core::services::proton::ProtonPayments;
use proton_api_core::services::proton::muon::client::flow::LoginExtraInfo;
use proton_api_core::services::proton::muon::util::BoxErrExt;
use proton_api_core::session::Config;
use proton_api_core::verification::ChallengeLoader;
use proton_api_core::verification::ChallengeNotifier;
use proton_api_core::verification::ChallengePayload;
use proton_api_core::verification::ChallengeResponse;
use proton_core_common::CoreAccountState;
use proton_core_common::OnSessionCloseNOP;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{KeyChain, KeyChainEntryKind, KeyChainError};
use proton_mail_common::MailContext;
use proton_mail_common::MailUserContext;
use proton_mail_common::context::{EventPollMode, ShouldInitializeMailUserContext};
use secrecy::{ExposeSecret, SecretString};
use std::error::Error as StdError;
use std::fs;
use std::io::{Result as IoResult, Write, stdin, stdout};
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

type Result<T, E = Box<dyn StdError>> = StdResult<T, E>;

const CACHE_SIZE: u64 = 1 << 20;

#[ctor::ctor]
fn init() {
    let filter = EnvFilter::from_default_env();

    fmt().with_env_filter(filter).pretty().init();
}

#[tokio::main]
async fn main() -> Result<()> {
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

    #[command(subcommand)]
    Payments(PaymentsCmd),
}

impl Cmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::Login(cmd) => cmd.run(ctx).await,
            Self::Payments(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Login to an account.
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

        _ = ctx
            .user_context_from_login_flow(&mut flow, OnSessionCloseNOP)
            .inspect_err(|err| error!("failed to create user context: {err:?}"))
            .await?;

        Ok(())
    }
}

/// Manage payments.
#[derive(Debug, Subcommand)]
enum PaymentsCmd {
    Subscription(SubscriptionPaymentsCmd),
}

impl PaymentsCmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        match self {
            Self::Subscription(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Display the active subscription for the given user.
#[derive(Debug, Args)]
struct SubscriptionPaymentsCmd {
    username: String,
}

impl SubscriptionPaymentsCmd {
    async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let plan = self
            .get_user_ctx(&ctx)
            .await?
            .api()
            .get_payments_subscription()
            .await?;

        println!("{plan:#?}");

        Ok(())
    }

    async fn get_user_ctx(&self, ctx: &Arc<MailContext>) -> Result<Arc<MailUserContext>> {
        for acc in ctx.get_accounts().await? {
            if acc.name_or_addr != self.username {
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
                .user_context_from_session(
                    &session,
                    None,
                    ShouldInitializeMailUserContext::Yes,
                    OnSessionCloseNOP,
                )
                .await?);
        }

        Err("account not found")?
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
    let keychain = Arc::new(kch);
    let notifier = Arc::new(HvNotifier::new(cfg.clone()));

    Ok(MailContext::new(
        dir.join("session"),
        dir.join("user"),
        dir.join("cache").join("core"),
        dir.join("cache").join("mail"),
        CACHE_SIZE,
        None,
        keychain,
        cfg,
        Some(notifier),
        None,
        EventPollMode::Manual,
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

        return Ok(ctx.resume_login_flow(acc.remote_id, session).await?);
    }

    Ok(ctx.new_login_flow().await?)
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

struct HvNotifier {
    cfg: Config,
}

impl HvNotifier {
    fn new(cfg: Config) -> Self {
        Self { cfg }
    }

    async fn handle_challenge(&self, challenge: ChallengePayload) -> Result<ChallengeResponse> {
        let _ = ChallengeLoader::new(self.cfg.clone())
            .await?
            .get(challenge.base(), challenge.path(), challenge.query(), [])
            .await?;

        Ok(ChallengeResponse::success("111111", "email"))
    }
}

#[async_trait]
impl ChallengeNotifier for HvNotifier {
    async fn on_challenge(&self, payload: ChallengePayload) -> ChallengeResponse {
        self.handle_challenge(payload)
            .inspect_err(|e| error!("failed to handle challenge: {e:?}"))
            .unwrap_or_else(|_| ChallengeResponse::Failure)
            .await
    }
}
