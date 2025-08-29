#![allow(clippy::print_stdout)]

use muon::env::EnvId;
use proton_account_api::login::LoginFlow;
use proton_account_api::shared::challenge::ChallengeInfo;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::ApiConfig;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::event_loop::EventPollMode;
use proton_core_common::migration_snooper::NoopMigrationSnooper;
use proton_core_common::os::{InMemoryKeyChain, KeyChainExt as _};
use proton_core_common::post_login_check::DefaultPostLoginValidator;
use proton_core_common::{Context, ContextBuilder, Origin};
use proton_log_service::LogService;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use std::io::{BufRead, Write, stdin, stdout};
use std::sync::Arc;
use tempdir::TempDir;
use tokio::runtime;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

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
                .parse_lossy("info,proton_core_api=debug,proton_mail_api_debug"),
        );

    tracing_subscriber::registry().with(file_subscriber).init();

    let user_email = std::env::var("USER_EMAIL").unwrap();
    let user_password = std::env::var("USER_PASSWORD").unwrap();
    let context = create_context().await;
    let session = Session::new().await.unwrap();
    let migration_snooper = Box::new(NoopMigrationSnooper);

    let post_login_validator = Box::new(DefaultPostLoginValidator::new(
        Some(2),
        Arc::clone(&context),
    ));

    let mut login_flow = LoginFlow::new(
        session.clone(),
        ChallengeInfo::default(),
        migration_snooper,
        post_login_validator,
    );

    login_flow
        .login_with_credentials(user_email, user_password, None)
        .await
        .unwrap();

    if login_flow.is_awaiting_2fa() {
        let mut line_reader = std::io::BufReader::new(stdin());
        {
            for _ in 0..3 {
                stdout()
                    .lock()
                    .write_all("Please Input TOTP:".as_bytes())
                    .unwrap();
                stdout().lock().flush().unwrap();

                let mut line = String::new();
                if line_reader.read_line(&mut line).is_err() {
                    eprintln!("Failed to read totp");
                    return;
                }

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

    let (user_id, session_id) = (login_flow.user_id(), login_flow.session_id());
    println!("User ID is {}", user_id.unwrap());
    println!("Session ID is {}", session_id.unwrap());

    let _ = session
        .get_conversations(GetConversationsOptions {
            page: 0,
            page_size: 10,
            label_id: LabelId::from("0".to_owned()).into(),
            desc: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();

    let messages = session
        .get_messages(GetMessagesOptions {
            page: 0,
            page_size: 10,
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap()
        .messages;

    for m in messages {
        let m = session.get_message(m.id).await.unwrap();
        println!("{:?}", m.message.body.attachments);
    }

    session.logout().await.unwrap();
}

async fn create_context() -> Arc<Context> {
    let tmp_dir = TempDir::new("account_test").expect("failed to create temp dir");
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
        ContextBuilder::new(),
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
    )
    .await
    .expect("failed to create core context")
}
