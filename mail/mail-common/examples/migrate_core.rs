use proton_core_api::auth::UserKeySecret;
use proton_core_api::session::EnvId;
use proton_core_api::store::UserData;
use proton_core_common::Origin;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::models::Label;
use proton_core_common::os::{InMemoryKeyChain, KeyChain, KeyChainExt};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::LogService;
use proton_mail_common::MailContext;
use proton_mail_common::context::ShouldInitializeMailUserContext;
use secrecy::SecretString;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime;
use tracing::level_filters::LevelFilter;
use tracing::{Level, info};
use tracing_subscriber::EnvFilter;

async fn prepare_context(tmp_dir: &TempDir) -> (Arc<MailContext>, Arc<dyn KeyChain>) {
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
    let keychain: Arc<dyn KeyChain> = Arc::new(keychain);

    let config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(tmp_dir.path().into())
        .build();

    let context = MailContext::new(
        Origin::App,
        runtime::Handle::current(),
        tmp_dir.path().join("session"),
        tmp_dir.path().join("user"),
        tmp_dir.path().join("core_cache"),
        tmp_dir.path().join("mail_cache"),
        50 * 1204 * 1024,
        Arc::clone(&keychain),
        ApiConfig::default_with_env(EnvId::new_atlas()),
        None,
        None,
        LogService::new(config),
        EventPollMode::Manual,
        Default::default(),
        Arc::new(NoopIssueReporter),
    )
    .await
    .unwrap();
    (context, keychain)
}

fn into_api_password_mode(
    mode: proton_core_common::datatypes::PasswordMode,
) -> proton_core_api::auth::PasswordMode {
    match mode {
        proton_core_common::datatypes::PasswordMode::One => {
            proton_core_api::auth::PasswordMode::One
        }
        proton_core_common::datatypes::PasswordMode::Two => {
            proton_core_api::auth::PasswordMode::Two
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .init();
    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();

    tracing::info!("Step 1. We simulate legacy app logging in and storing session in the DB");

    let legacy_dir = TempDir::new().unwrap();
    let (legacy_context, legacy_key_chain) = prepare_context(&legacy_dir).await;

    let mut flow = legacy_context.new_login_flow().await.unwrap();

    flow.login_with_credentials(user_email, user_password, None)
        .await
        .unwrap();

    let ctx = legacy_context
        .user_context_from_login_flow(&mut flow)
        .await
        .unwrap();

    let legacy_session = legacy_context
        .get_session(ctx.session_id().clone())
        .await
        .unwrap()
        .unwrap();

    // Test that session works in legacy
    let network_session = ctx.session();
    let labels = Label::all_labels(network_session).await.unwrap();
    tracing::info!("Legacy labels: {labels:?}");

    tracing::info!(
        "Step 2. We simulate our ET app retrieving data from keychain + blob plist and decrypting it"
    );

    let user_id = ctx.user_id().to_owned();
    let session_id = ctx.session_id().to_owned();
    let account = ctx.user_context().core_account().await.unwrap();

    let legacy_encryption_key = legacy_key_chain
        .load::<SessionEncryptionKey>()
        .unwrap()
        .unwrap();

    let decrypted_key_secret = legacy_encryption_key
        .decrypt(&*legacy_session.key_secret.clone().unwrap())
        .unwrap();

    let decrypted_refresh_token = SecretString::new(
        String::from_utf8(
            legacy_encryption_key
                .decrypt(&*legacy_session.refresh_token)
                .unwrap(),
        )
        .unwrap(),
    );

    let user_data = UserData {
        username: account.username.unwrap(),
        display_name: account.display_name.unwrap(),
        primary_addr: account.primary_addr.unwrap(),
        key_secret: UserKeySecret::from(decrypted_key_secret),
        password_mode: into_api_password_mode(account.password_mode.unwrap()).into(),
    };

    drop(ctx);
    drop(flow);
    drop(legacy_context);

    tracing::info!("Step 3. We create a new login flow, simulating migration");
    let et_dir = TempDir::new().unwrap();
    let (et_context, _et_key_chain) = prepare_context(&et_dir).await;
    let mut flow = et_context.new_login_flow().await.unwrap();
    flow.migrate(user_id, session_id, user_data, decrypted_refresh_token)
        .await
        .unwrap();

    let session_id = flow.session_id().unwrap().clone();

    tracing::info!("=== Finished migrating ===");
    let session = et_context.get_session(session_id).await.unwrap().unwrap();

    let ctx = et_context
        .user_context_from_session(&session, ShouldInitializeMailUserContext::Yes)
        .await
        .unwrap();

    tracing::info!("Testing if the migration works. Let's fetch some labels");

    let network_session = ctx.session();
    let labels = Label::all_labels(network_session).await.unwrap();

    tracing::info!("ET Labels: {labels:?}");
}
