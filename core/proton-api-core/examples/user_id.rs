use proton_api_core::auth::{new_arc_auth_store, InMemoryStore};
use proton_api_core::{http, ping};
use proton_api_core::{Session, SessionType};
pub use tokio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
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
                .parse_lossy("info,proton_api_core=debug"),
        );
    tracing_subscriber::registry().with(file_subscriber).init();
    let user_email = std::env::var("PAPI_USER_EMAIL").unwrap();
    let user_password = std::env::var("PAPI_USER_PASSWORD").unwrap();
    let app_version = std::env::var("PAPI_APP_VERSION").unwrap();

    let client = http::ClientBuilder::new()
        .app_version(&app_version)
        .debug()
        .build()
        .unwrap();

    let auth_store = new_arc_auth_store(InMemoryStore::default());

    ping(&client).await.unwrap();

    let session = match Session::login(
        client,
        auth_store.clone(),
        &user_email,
        &user_password,
        None,
    )
    .await
    .unwrap()
    {
        SessionType::Authenticated(c) => c,

        SessionType::AwaitingTotp(t) => {
            let mut stdout = tokio::io::stdout();
            let mut line_reader = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            let session = {
                let mut session = None;
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

                    match t.submit_totp(totp).await {
                        Ok(ac) => {
                            session = Some(ac);
                            break;
                        }
                        Err(e) => {
                            eprintln!("Failed to submit totp: {e}");
                            continue;
                        }
                    }
                }

                session
            };

            let Some(c) = session else {
                eprintln!("Failed to pass TOTP 2FA auth");
                return;
            };
            c
        }
    };

    let user = session.get_user().await.unwrap();
    println!("User ID is {}", user.id);

    let settings = session.get_user_settings().await.unwrap();
    println!("User settings is {:?}", settings);

    session.logout().await.unwrap();
}
