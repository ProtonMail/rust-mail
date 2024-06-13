use proton_api_core::auth::{new_arc_auth_store, InMemoryStore};
use proton_api_core::exports::tracing::level_filters::LevelFilter;
use proton_api_core::http::APIEnvConfig;
use proton_api_core::login::Flow;
use proton_api_core::{http, Session};
use proton_api_mail::domain::MessageMetadataFilterBuilder;
use proton_api_mail::MailSession;
use std::io::{stdin, stdout, BufRead, Write};
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
                .parse_lossy("info,proton_api_core=debug,proton_api_mail_debug"),
        );
    tracing_subscriber::registry().with(file_subscriber).init();
    let user_email = std::env::var("USER_EMAIL").unwrap();
    let user_password = std::env::var("USER_PASSWORD").unwrap();
    let api_env_config = APIEnvConfig::default();

    let client = http::Builder::new()
        .api_env_config(api_env_config)
        .debug()
        .build()
        .unwrap();

    let auth_store = new_arc_auth_store(InMemoryStore::default());
    let session = Session::new(client, auth_store);

    let mut login_flow = Flow::new(session.clone());
    login_flow
        .login(&user_email, &user_password, None)
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
                };

                let totp = line.trim_end_matches('\n');

                match login_flow.submit_totp(totp).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("Failed to submit totp: {e}");
                        continue;
                    }
                }
            }
        };
    }

    let user = login_flow.reset_and_take_user().unwrap();
    println!("User ID is {}", user.id);

    let mail_session = MailSession::new(session.clone());

    let messages = mail_session
        .message_metadata(MessageMetadataFilterBuilder::new(0, 10).build())
        .await
        .unwrap()
        .messages;

    for m in messages {
        let m = mail_session.message(&m.id).await.unwrap();
        println!("{:?}", m.attachments);
    }

    session.logout().await.unwrap();
}
