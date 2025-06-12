use std::sync::Arc;

use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_api::session::Config;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_mail_common::MailContext;
use tempdir::TempDir;
use tracing::level_filters::LevelFilter;
use tracing::{Level, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .init();
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
            "",
            None,
            EventPollMode::Manual,
        )
        .await
        .unwrap()
    };

    let mut flow = context.new_login_flow().await.unwrap();

    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();
    flow.login(user_email, user_password, LoginExtraInfo::default())
        .await
        .unwrap();

    context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();
}
