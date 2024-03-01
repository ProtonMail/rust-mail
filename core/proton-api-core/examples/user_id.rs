use proton_api_core::auth::{new_arc_auth_store, InMemoryStore};
use proton_api_core::login::LoginFlow;
use proton_api_core::{http, Session};
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
    let session = Session::new(client, auth_store);

    let mut login_flow = LoginFlow::new(session.clone());
    login_flow
        .login(&user_email, &user_password, None)
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

    let settings = session.get_user_settings().await.unwrap();
    println!("User settings is {:?}", settings);

    session.logout().await.unwrap();
}
