use clap::Parser;
use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_api::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_core_common::{Context, CoreContextError};
use std::sync::Arc;
use tempdir::TempDir;
use tracing::Level;

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
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .init();

    let dir = TempDir::new("core-common").unwrap();
    let session_db_dir = dir.path().join("sessions");
    let user_db_dir = dir.path().join("users");
    let cache_dir = dir.path().join("cache");

    let key = SessionEncryptionKey::random();
    let key_chain = InMemoryKeyChain::default();
    key_chain.store(key).unwrap();

    let config = Config::default();
    let context = Context::new(
        session_db_dir,
        user_db_dir,
        Arc::new(key_chain),
        [],
        config,
        None,
        cache_dir,
        None,
        None,
    )
    .await
    .unwrap();

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

    assert!(matches!(
        context
            .user_context_from_login_flow(&mut flow)
            .await
            .unwrap_err(),
        CoreContextError::DuplicateContext(_)
    ));

    // Creating the context from a session will work.
    let sessions = context
        .get_account_sessions(ctx.user_id().clone())
        .await
        .unwrap();
    let ctx2 = context
        .user_context_from_session(&sessions[0], None)
        .await
        .unwrap();

    assert!(Arc::ptr_eq(&ctx2, &ctx));
}
