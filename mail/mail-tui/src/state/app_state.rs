use crate::app::{AppEventHandler, AppLocalDispatcher};
use crate::events::AppEvent;
use crate::keychain::AppKeyChain;
use crate::state::login_state::LoginState;
use crate::state::mailbox_state::MailboxState;
use crate::state::session_state::SessionState;
use anyhow::anyhow;
use proton_async::runtime;
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing;
use proton_mail_common::MailContext;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub struct AppState {
    pub mail_context: MailContext,
    pub login_state: LoginState,
    pub mailbox_state: MailboxState,
    pub session_state: SessionState,
}

pub const APP_ID: &str = "com.proton.proton-mail-tui";
impl AppState {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let cache_dir = dirs::cache_dir()
            .ok_or(anyhow!("Failed to get cache dir"))?
            .join(APP_ID);

        let config_dir = dirs::config_dir()
            .ok_or(anyhow!("Failed to get config dir"))?
            .join(APP_ID);

        std::fs::create_dir_all(&cache_dir)?;
        std::fs::create_dir_all(&config_dir)?;

        let log_file = cache_dir.join("app.log");
        init_log(log_file)?;

        tracing::info!("Creating Async Runtime...");
        let mut keychain = AppKeyChain::new()?;
        keychain.init()?;
        let runtime = runtime::MTRuntime::new(4)?;
        let context = MailContext::new(runtime, config_dir, cache_dir, Arc::new(keychain), None)?;

        Ok(Self {
            mail_context: context,
            login_state: LoginState::LoggedOut,
            mailbox_state: MailboxState::new(),
            session_state: SessionState::new(),
        })
    }
}
impl AppEventHandler<AppState, AppEvent> for AppState {
    fn on_event(&mut self, dispatcher: AppLocalDispatcher<AppState, AppEvent>, event: AppEvent) {
        tracing::trace!("Handling Event: {:?}", event);
        match event {
            AppEvent::Login(event) => {
                self.login_state
                    .handle_event(dispatcher, &self.mail_context, event)
            }
            AppEvent::Mailbox(event) => {
                self.mailbox_state.handle_event(dispatcher, event);
            }
            AppEvent::Session(event) => {
                self.session_state
                    .handle_event(dispatcher, event, &self.mail_context);
            }
        }
    }
}

pub fn app_tracing_env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse_lossy(
            "info,proton_mail_tui=debug,proton_mail_db=trace,proton_sqlite3=trace,\
                    proton_core_db=trace,proton_core_common=trace,proton_mail_common=trace,\
                    proton_event_loop=trace,proton_api_core=trace,proton_action_queue=trace",
        )
}

fn init_log(log_path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    let log_file = std::fs::File::create(log_path)?;
    let file_subscriber = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(app_tracing_env_filter());
    let tui_log_subscriber =
        tui_logger::tracing_subscriber_layer().with_filter(app_tracing_env_filter());
    tui_logger::set_default_level(log::LevelFilter::Debug);
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(tui_log_subscriber)
        .init();
    Ok(())
}
