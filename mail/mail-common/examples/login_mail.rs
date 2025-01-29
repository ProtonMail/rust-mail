#![allow(clippy::print_stdout)]
use futures::TryFutureExt;
use proton_api_core::services::proton::muon::client::flow::LoginExtraInfo;
use proton_api_core::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::{MailContext, MailUserContext};
use std::io::{stdin, stdout, Result as IoResult, Write};
use std::path::Path;
use std::sync::Arc;
use tempdir::TempDir;
use tracing::{error, info};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let dir = TempDir::new("login")?.into_path();
    let key = SessionEncryptionKey::random();
    let kch = InMemoryKeyChain::default();
    let cfg = Config::default();

    kch.store(key.to_base64())?;

    let mail_ctx = new_mail_ctx(&dir, kch.into(), cfg).await?;
    let user_ctx = new_user_ctx(Arc::clone(&mail_ctx)).await?;

    println!("{:#?}", user_ctx.user().await?);

    let account = mail_ctx
        .get_account(user_ctx.user_id().to_owned())
        .await?
        .unwrap();

    println!("{account:#?}");

    Ok(())
}

async fn new_mail_ctx(
    dir: &Path,
    kch: Arc<InMemoryKeyChain>,
    cfg: Config,
) -> Result<Arc<MailContext>> {
    let session = dir.join("session");
    let user = dir.join("user");
    let cache_path = dir.join("cache");
    let cache_size = 1 << 20;

    Ok(MailContext::new(
        session,
        user,
        cache_path.join("core"),
        cache_path.join("mail"),
        cache_size,
        kch,
        cfg,
        None,
    )
    .await?)
}

async fn new_user_ctx(ctx: Arc<MailContext>) -> Result<Arc<MailUserContext>> {
    let mut flow = ctx.new_login_flow()?;

    flow.login(
        read("username")?,
        read("password")?,
        LoginExtraInfo::default(),
    )
    .inspect_err(|err| error!("failed to login: {err}"))
    .await?;

    if flow.is_awaiting_2fa() {
        flow.submit_totp(read("2nd factor")?)
            .inspect_err(|err| error!("failed to submit TOTP: {err}"))
            .await?;
    }

    if flow.is_awaiting_mailbox_password() {
        flow.submit_mailbox_password(read("2nd password")?)
            .inspect_err(|err| error!("failed to submit mailbox password: {err}"))
            .await?;
    }

    let user_ctx = ctx
        .user_context_from_login_flow(&mut flow)
        .inspect_err(|err| error!("failed to create user context: {err}"))
        .await?;

    MailUserContext::initialize_async(user_ctx.clone(), &InitCb)
        .inspect_err(|(stage, err)| error!("user init failed at stage {stage:?}: {err}"))
        .map_err(|(_, err)| err)
        .await?;

    Ok(user_ctx)
}

fn read(prompt: &str) -> IoResult<String> {
    print!("{prompt}: ");
    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    Ok(input.trim().to_owned())
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
