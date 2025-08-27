use std::sync::Arc;

use proton_core_api::session::EnvId;
use proton_core_common::Origin;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt};
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use tempdir::TempDir;
use tokio::runtime;
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
        )
        .await
        .unwrap()
    };

    let mut flow = context.new_login_flow().await.unwrap();

    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();
    flow.login_with_credentials(user_email, user_password, None)
        .await
        .unwrap();

    context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();
}
