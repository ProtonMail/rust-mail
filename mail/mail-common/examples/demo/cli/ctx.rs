use crate::cli::APP_NAME;
use anyhow::Result;
use log_service::{Config as LogConfig, LogService};
use proton_account_api::login::LoginFlow;
use proton_core_api::session::Config;
use proton_core_api::verification::ChallengeNotifier;
use proton_core_common::CoreAccountState;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::KeyChain;
use proton_mail_common::context::ShouldInitializeMailUserContext as Init;
use proton_mail_common::{MailContext, MailUserContext};
use std::path::Path;
use std::sync::Arc;

pub async fn new_mail_ctx<K, N>(
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

    let config = LogConfig::builder()
        .name(APP_NAME.to_owned())
        .directory(dir.join("logs"))
        .build();

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
        LogService::new(config),
        EventPollMode::Manual,
    )
    .await?)
}

pub trait MailContextExt {
    async fn get_user_ctx(&self, username: &str) -> Result<Arc<MailUserContext>>;

    async fn new_login_flow(&self, username: Option<&str>) -> Result<LoginFlow>;
}

impl MailContextExt for Arc<MailContext> {
    async fn get_user_ctx(&self, username: &str) -> Result<Arc<MailUserContext>> {
        get_user_ctx(self, username).await
    }

    async fn new_login_flow(&self, username: Option<&str>) -> Result<LoginFlow> {
        new_login_flow(self, username).await
    }
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
            .user_context_from_session(&session, None, Init::Yes)
            .await?);
    }

    Err(anyhow!("account not found"))
}

async fn new_login_flow(ctx: &MailContext, username: Option<&str>) -> Result<LoginFlow> {
    for acc in ctx.get_accounts().await? {
        if let Some(username) = username {
            if username != acc.name_or_addr {
                continue;
            }
        }

        let session = match ctx.get_account_state(acc.remote_id.clone()).await? {
            Some(CoreAccountState::LoggedIn(_)) => Err(anyhow!("account already logged in"))?,
            Some(CoreAccountState::NeedMbp(mut s)) => s.pop().unwrap(),
            Some(CoreAccountState::NeedTfa(mut s)) => s.pop().unwrap(),
            _ => continue,
        };

        return Ok(ctx.resume_login_flow(acc.remote_id, session).await?);
    }

    Ok(ctx.new_login_flow().await?)
}
