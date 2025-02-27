use proton_api_core::services::proton::muon::client::flow::LoginExtraInfo;
use proton_api_core::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_core_common::Context;
use std::sync::Arc;
use tempdir::TempDir;
use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .init();
    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();

    let dir = TempDir::new("core-common").unwrap();
    let session_db_dir = dir.path().join("sessions");
    let user_db_dir = dir.path().join("users");
    let cache_dir = dir.path().join("cache");

    let key = SessionEncryptionKey::random().to_base64();
    let key_chain = Arc::new(InMemoryKeyChain::default());
    key_chain.store(key).unwrap();

    let config = Config::default();
    let context = Context::new(
        session_db_dir,
        user_db_dir,
        key_chain,
        [],
        config,
        cache_dir,
        1024 * 1024,
    )
    .await
    .unwrap();

    let mut flow = context.new_login_flow().unwrap();

    flow.login(user_email, user_password, LoginExtraInfo::default())
        .await
        .unwrap();

    context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();
}
