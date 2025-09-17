use crate::cli::APP_NAME;
use anyhow::Result;
use proton_account_api::login::LoginFlow;
use proton_core_api::services::proton::muon::util::DurationExt;
use proton_core_api::verification::ChallengeNotifier;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::CoreAccount;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::KeyChain;
use proton_core_common::{CoreAccountState, Origin};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::{Config as LogConfig, LogService};
use proton_mail_common::context::ShouldInitializeMailUserContext as Init;
use proton_mail_common::{MailContext, MailUserContext};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime;

pub async fn new_mail_ctx<K, N>(
    dir: &Path,
    cfg: ApiConfig,
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
        Origin::App,
        runtime::Handle::current(),
        dir.join("session"),
        dir.join("user"),
        dir.join("cache").join("core"),
        dir.join("cache").join("mail"),
        CACHE_SIZE,
        kch,
        cfg,
        Some(hvn),
        None,
        LogService::new(config),
        EventPollMode::Automatic(30.s()),
        Default::default(),
        Arc::new(NoopIssueReporter),
    )
    .await?)
}

pub trait MailContextExt {
    async fn get_user_ctx(&self, username: &str) -> Result<Arc<MailUserContext>>;

    async fn new_or_resume_login_flow(&self, username: Option<&str>) -> Result<LoginFlow>;
}

impl MailContextExt for Arc<MailContext> {
    async fn get_user_ctx(&self, username: &str) -> Result<Arc<MailUserContext>> {
        get_user_ctx(self, username).await
    }

    async fn new_or_resume_login_flow(&self, username: Option<&str>) -> Result<LoginFlow> {
        new_or_resume_login_flow(self, username).await
    }
}

async fn get_user_ctx(ctx: &Arc<MailContext>, username: &str) -> Result<Arc<MailUserContext>> {
    for acc in ctx.get_accounts().await? {
        if !match_account(&acc, username) {
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

        return Ok(ctx.user_context_from_session(&session, Init::Yes).await?);
    }

    Err(anyhow!("account not found"))
}

async fn new_or_resume_login_flow(ctx: &MailContext, username: Option<&str>) -> Result<LoginFlow> {
    for acc in ctx.get_accounts().await? {
        if let Some(username) = username
            && !match_account(&acc, username)
        {
            continue;
        }

        let session = match ctx.get_account_state(acc.remote_id.clone()).await? {
            Some(CoreAccountState::NeedNewPass(mut s)) => s.pop().unwrap(),
            Some(CoreAccountState::NeedMbp(mut s)) => s.pop().unwrap(),
            Some(CoreAccountState::NeedTfa(mut s)) => s.pop().unwrap(),
            _ => continue,
        };

        return Ok(ctx.resume_login_flow(acc.remote_id, session).await?);
    }

    Ok(ctx.new_login_flow().await?)
}

fn match_account(account: &CoreAccount, query: &str) -> bool {
    if account
        .username
        .as_deref()
        .is_some_and(|name| name == query)
    {
        return true;
    }

    if account
        .primary_addr
        .as_deref()
        .is_some_and(|addr| addr == query)
    {
        return true;
    }

    account.name_or_addr == query
}
