use crate::app::AppDispatcher;
use crate::events::AppEvents;
use crate::state::login_state::LoginState;
use crate::state::mailbox_state::MailboxState;
use anyhow::anyhow;
use log::error;
use proton_api_mail::proton_api_core::exports::tracing;
use proton_async::runtime;
use secrecy::SecretString;
use std::error::Error;
use std::path::{Path, PathBuf};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub struct AppState {
    pub runtime: runtime::MTRuntime,
    pub db_path: PathBuf,
    pub login_state: LoginState,
    pub mailbox_state: MailboxState,
}

const APP_ID: &str = "com.proton.proton-mail-tui";
impl AppState {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let cache_dir = dirs::cache_dir()
            .ok_or(anyhow!("Failed to get cache dir"))?
            .join(APP_ID);

        let db_dir = cache_dir.join("db");
        std::fs::create_dir_all(&db_dir)?;

        let log_file = cache_dir.join("app.log");
        init_log(log_file)?;

        tracing::info!("Creating Async Runtime...");
        let runtime = runtime::MTRuntime::new()?;

        Ok(Self {
            runtime,
            db_path: db_dir,
            login_state: LoginState::LoggedOut,
            mailbox_state: MailboxState::new(),
        })
    }
    pub fn login(
        &mut self,
        dispatcher: AppDispatcher<AppState, AppEvents>,
        email: String,
        password: SecretString,
    ) {
        self.login_state.login(
            dispatcher,
            &self.runtime,
            self.db_path.clone(),
            email,
            password,
        )
    }

    pub fn submit_2fa(&self, dispatcher: AppDispatcher<AppState, AppEvents>, code: String) {
        self.login_state
            .submit_2fa(dispatcher, &self.runtime, self.db_path.clone(), code);
    }

    pub fn logout(&mut self) {
        self.login_state.logout(&self.runtime);
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        match &self.login_state {
            LoginState::LoggedOut => {}
            LoginState::AwaitingTotp(s) => {
                let s = s.clone();
                self.runtime.block_on(async {
                    if let Err(e) = s.logout().await {
                        error!("Failed to logout :{e}");
                    }
                });
            }
            LoginState::LoggedIn(s) => {
                let s = s.session.session().clone();
                self.runtime.block_on(async {
                    if let Err(e) = s.logout().await {
                        error!("Failed to logout :{e}");
                    }
                });
            }
        }
    }
}

fn init_log(log_path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let log_file = std::fs::File::create(log_path)?;
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::TRACE.into())
                .parse_lossy(
                    "info,proton_mail_tui=debug,proton_mail_db=trace,proton_sqlite3=trace",
                ),
        );
    tracing_subscriber::registry().with(file_subscriber).init();
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum DataLoadError {
    #[error("DB: {0}")]
    DB(#[from] proton_mail_db::DBError),
    #[error("DB: {0}")]
    DBMigration(#[from] proton_mail_db::DBMigrationError),
    #[error("HTTP: {0}")]
    Http(#[from] proton_api_mail::proton_api_core::http::HttpRequestError),
    #[error("Unexpected: {0}")]
    Other(#[from] anyhow::Error),
}
