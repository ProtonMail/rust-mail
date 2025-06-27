// cargo run --example context_reuse -- --username free --password free

use clap::Parser;
use log_service::LogService;
use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_api::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_mail_common::context::ShouldInitializeMailUserContext;
use proton_mail_common::{MailContext, MailContextError};
use std::sync::Arc;
use tempdir::TempDir;
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
    let tmp_dir = TempDir::new("cli").unwrap();
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
        let config = log_service::Config::builder()
            .name("log".into())
            .directory(tmp_dir.path().into())
            .build();

        MailContext::new(
            tmp_dir.path().join("session"),
            tmp_dir.path().join("user"),
            tmp_dir.path().join("core_cache"),
            tmp_dir.path().join("mail_cache"),
            50 * 1204 * 1024,
            None,
            Arc::new(keychain),
            Config::atlas(),
            None,
            None,
            LogService::new(config),
            EventPollMode::Manual,
        )
        .await
        .unwrap()
    };

    let mut flow = context.new_login_flow().await.unwrap();

    flow.login(
        username.clone(),
        password.clone(),
        LoginExtraInfo::default(),
    )
    .await
    .unwrap();

    let ctx = context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();

    // Create a new login for this context will fail.
    let mut flow = context.new_login_flow().await.unwrap();

    flow.login(username, password, LoginExtraInfo::default())
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
        .user_context_from_session(&sessions[0], None, ShouldInitializeMailUserContext::Yes)
        .await
        .unwrap();

    assert!(Arc::ptr_eq(&ctx2, &ctx));
}
