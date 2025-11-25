// cargo run --example context_reuse -- --username free --password free

use clap::Parser;
use proton_core_api::session::EnvId;
use proton_core_common::Origin;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::LogService;
use proton_mail_common::{MailContext, MailContextError, ShouldInitializeMailUserContext};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    username: String,
    #[arg(short, long)]
    password: String,
}
#[tokio::main]
async fn main() {
    let Args { username, password } = Args::parse();
    let tmp_dir = TempDir::new().unwrap();
    let context = {
        let env_filter = EnvFilter::builder()
            .with_default_directive(LevelFilter::TRACE.into())
            .parse_lossy(
                "info,proton_sqlite3=trace,\
                        proton_core_common=trace,proton_mail_common=trace,\
                        proton_event_loop=trace,proton_core_api=trace,\
                        proton_action_queue=trace,proton_mail_api=trace,\
                        stash=error",
            );
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(env_filter)
            .init();
        info!("TEMP_DIR = {tmp_dir:?}");

        let keychain = InMemoryKeyChain::default();
        let key = SessionEncryptionKey::random();
        keychain.store(key).unwrap();
        let config = proton_log_service::Config::builder()
            .name("log".into())
            .directory(tmp_dir.path().into())
            .build();

        MailContext::new(
            Origin::App,
            runtime::Handle::current(),
            tmp_dir.path().join("session"),
            tmp_dir.path().join("user"),
            tmp_dir.path().join("core_cache"),
            tmp_dir.path().join("mail_cache"),
            50 * 1204 * 1024,
            Arc::new(keychain),
            ApiConfig::default_with_env(EnvId::new_atlas()),
            None,
            None,
            LogService::new(config),
            EventPollMode::Manual,
            Default::default(),
            Arc::new(NoopIssueReporter),
        )
        .await
        .unwrap()
    };

    let mut flow = context.new_login_flow().await.unwrap();

    flow.login_with_credentials(username.clone(), password.clone(), None)
        .await
        .unwrap();

    let ctx = context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();

    // Create a new login for this context will fail.
    let mut flow = context.new_login_flow().await.unwrap();

    flow.login_with_credentials(username, password, None)
        .await
        .unwrap();

    match context.user_context_from_login_flow(&mut flow).await {
        Ok(_) => panic!("Expected MailContextError::DuplicateContext"),
        Err(MailContextError::DuplicateContext(_)) => { /* Ok */ }
        _ => panic!("Expected MailContextError::DuplicateContext"),
    }

    // Creating the context from a session will work.
    let sessions = context
        .get_account_sessions(ctx.user_id().clone())
        .await
        .unwrap();
    let ctx2 = context
        .user_context_from_session(&sessions[0], ShouldInitializeMailUserContext::Yes)
        .await
        .unwrap();

    assert!(Arc::ptr_eq(&ctx2, &ctx));
}
