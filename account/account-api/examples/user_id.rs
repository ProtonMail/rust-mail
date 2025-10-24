#![allow(clippy::print_stdout)]

use muon::env::EnvId;
use proton_account_api::login::LoginFlow;
use proton_account_api::shared::challenge::ChallengeInfo;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::session::Session;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::migration_snooper::NoopMigrationSnooper;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt as _};
use proton_core_common::post_login_check::DefaultPostLoginValidator;
use proton_core_common::{Context, Origin};
use proton_issue_reporter_service::NoopIssueReporter;
use proton_log_service::LogService;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::runtime;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

//#[tokio::main(worker_threads = 1)]
#[tokio::main]
async fn main() {
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .with_ansi(false)
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::TRACE.into())
                .parse_lossy("info,proton_core_api=debug"),
        );
    tracing_subscriber::registry().with(file_subscriber).init();
    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();
    let app_platform = std::env::var("PAPI_APP_PLATFORM").unwrap();
    let app_product = std::env::var("PAPI_APP_PRODUCT").unwrap();
    let app_version = std::env::var("PAPI_APP_VERSION").unwrap();

    let context = create_context().await;

    let session = Session::builder()
        .with_app_version(app_platform, app_product, app_version)
        .build()
        .await
        .unwrap();

    let mut login_flow = LoginFlow::new(
        session.clone(),
        ChallengeInfo::default(),
        Box::new(NoopMigrationSnooper),
        Box::new(DefaultPostLoginValidator::new(Some(2), context)),
    );
    login_flow
        .login_with_credentials(user_email, user_password, None)
        .await
        .unwrap();

    if login_flow.is_awaiting_2fa() {
        let mut stdout = tokio::io::stdout();
        let mut line_reader = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        {
            for _ in 0..3 {
                stdout
                    .write_all("Please Input TOTP:".as_bytes())
                    .await
                    .unwrap();
                stdout.flush().await.unwrap();

                let Some(line) = line_reader.next_line().await.unwrap() else {
                    eprintln!("Failed to read totp");
                    return;
                };

                let totp = line.trim_end_matches('\n');

                match login_flow.submit_totp(totp.to_owned()).await {
                    Ok(()) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("Failed to submit totp: {e}");
                    }
                }
            }
        };
    }

    if login_flow.is_awaiting_mailbox_password() {
        let mut stdout = tokio::io::stdout();
        let mut line_reader = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        {
            for _ in 0..3 {
                stdout
                    .write_all("Please type the mailbox password:".as_bytes())
                    .await
                    .unwrap();
                stdout.flush().await.unwrap();

                let Some(line) = line_reader.next_line().await.unwrap() else {
                    eprintln!("Failed to read mailbox password");
                    return;
                };

                let mailbox_pw = line.trim_end_matches('\n').to_owned();

                match login_flow.submit_mailbox_password(mailbox_pw).await {
                    Ok(()) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("Failed to submit maibix password: {e}");
                    }
                }
            }
        };
    }

    let (user_id, session_id) = (login_flow.user_id(), login_flow.session_id());
    println!("User ID is {}", user_id.unwrap());
    println!("Session ID is {}", session_id.unwrap());

    let settings = session.get_settings().await.unwrap();
    println!("User settings is {settings:?}");

    session.logout().await.unwrap();
}

async fn create_context() -> Arc<Context> {
    let tmp_dir = TempDir::new().expect("failed to create temp dir");
    let keychain = Arc::new(InMemoryKeyChain::default()).clone();
    let key = SessionEncryptionKey::random();
    let log_config = proton_log_service::Config::builder()
        .name("log".into())
        .directory(tmp_dir.path().into())
        .build();

    keychain
        .store(key.clone())
        .expect("failed to store in keychain");

    Context::new(
        Origin::App,
        runtime::Handle::current(),
        tmp_dir.path(),
        tmp_dir.path(),
        Arc::new(InMemoryKeyChain::default()).clone(),
        vec![],
        ApiConfig::default_with_env(EnvId::new_atlas()),
        None,
        None,
        tmp_dir.path().join("core-cache"),
        LogService::new(log_config),
        EventPollMode::Manual,
        #[allow(clippy::default_trait_access)]
        Default::default(),
        Arc::new(NoopIssueReporter),
        |e| e,
    )
    .await
    .expect("failed to create core context")
}
