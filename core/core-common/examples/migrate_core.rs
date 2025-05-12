use std::sync::Arc;

use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::muon::client::flow::{LoginExtraInfo, LoginFlowData};
use proton_core_api::session::{Config, CoreSession as _};
use proton_core_api::store::UserData;
use proton_core_common::Context;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::models::Label;
use proton_core_common::os::{InMemoryKeyChain, KeyChain, KeyChainExt};
use secrecy::SecretString;
use tempdir::TempDir;
use tracing::Level;

async fn prepare_context(dir: &TempDir) -> (Arc<Context>, Arc<dyn KeyChain>) {
    let session_db_dir = dir.path().join("sessions");
    let user_db_dir = dir.path().join("users");
    let cache_dir = dir.path().join("cache");

    let key = SessionEncryptionKey::random();
    let key_chain = InMemoryKeyChain::default();
    key_chain.store(key).unwrap();

    let key_chain: Arc<dyn KeyChain> = Arc::new(key_chain);
    let config = Config::default();
    let context = Context::new(
        session_db_dir,
        user_db_dir,
        Arc::clone(&key_chain),
        [],
        config,
        None,
        cache_dir,
        None,
        None,
    )
    .await
    .unwrap();

    (context, key_chain)
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

    let legacy_dir = TempDir::new("core-common-legacy").unwrap();
    let (legacy_context, legacy_key_chain) = prepare_context(&legacy_dir).await;

    let mut flow = legacy_context.new_login_flow().await.unwrap();

    flow.login(user_email, user_password, LoginExtraInfo::default())
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
    let network_session = ctx.session().api();
    let labels = Label::all_labels(network_session).await.unwrap();
    tracing::info!("Legacy labels: {labels:?}");

    tracing::info!(
        "Step 2. We simulate our ET app retrieving data from keychain + blob plist and decrypting it"
    );
    let user_id = ctx.user_id();
    let account = ctx.core_account().await.unwrap();

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

    let password_mode = into_api_password_mode(account.password_mode.unwrap());

    let login_flow_data = LoginFlowData {
        user_id: user_id.to_string(),
        session_id: ctx.session_id().to_string(),
        password_mode,
    };
    let user_data = UserData {
        username: account.username.unwrap(),
        display_name: account.display_name.unwrap(),
        primary_addr: account.primary_addr.unwrap(),
        key_secret: UserKeySecret::from(decrypted_key_secret),
    };

    drop(ctx);
    drop(flow);
    drop(legacy_context);

    tracing::info!("Step 3. We create a new login flow, simulating migration");
    let et_dir = TempDir::new("core-common-et").unwrap();
    let (et_context, _et_key_chain) = prepare_context(&et_dir).await;
    let mut flow = et_context.new_login_flow().await.unwrap();
    flow.migrate(user_data, login_flow_data, decrypted_refresh_token)
        .await
        .unwrap();

    let session_id = flow.session_id().unwrap().clone();

    tracing::info!("=== Finished migrating ===");
    let session = et_context.get_session(session_id).await.unwrap().unwrap();

    let ctx = et_context
        .user_context_from_session(&session, None)
        .await
        .unwrap();

    tracing::info!("Testing if the migration works. Let's fetch some labels");

    let network_session = ctx.session().api();
    let labels = Label::all_labels(network_session).await.unwrap();

    tracing::info!("ET Labels: {labels:?}");
}
