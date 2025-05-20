#![allow(clippy::print_stdout)]
use muon::client::flow::LoginExtraInfo;
use proton_core_api::login::Flow;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::{CoreSession, Session};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use std::io::{BufRead, Write, stdin, stdout};
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
    let session = Session::new().await.unwrap();

    let mut login_flow = Flow::new(session.clone());
    login_flow
        .login(user_email, user_password, LoginExtraInfo::default())
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
        .api()
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
        .api()
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
        let m = session.api().get_message(m.id).await.unwrap();
        println!("{:?}", m.message.body.attachments);
    }

    session.logout().await.unwrap();
}
