#![allow(clippy::large_enum_variant)]
mod app;
mod app_model;
mod keychain;
mod messages;
mod widgets;

use crate::app::App;
use clap::Parser;
use proton_core_common::datatypes::{ApiConfig, AppDetails};
use proton_mail_api::proton_core_api::session::EnvId;

use crate::app_model::AppModel;
use crossterm::ExecutableCommand;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{Stdout, stdout};
use std::path::PathBuf;
use std::sync::LazyLock;

type TerminalType = Terminal<CrosstermBackend<Stdout>>;

use zeroizing_alloc::ZeroAlloc;

#[global_allocator]
static ALLOC: ZeroAlloc<std::alloc::System> = ZeroAlloc(std::alloc::System);

fn initialize_panic_handler() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        crossterm::execute!(std::io::stderr(), LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}

#[derive(Parser, Clone, Debug)]
#[command(name = "proton-mail-tui")]
struct CliArgs {
    /// Which API to connect to, defaults to production
    #[arg(long, short)]
    environment: Option<String>,

    /// Used to identify the app in the API
    #[arg(long, default_value = "ios")]
    platform: String,

    /// Used to identify the app in the API
    #[arg(long, default_value = "mail")]
    product: String,

    /// Used to identify the app in the API
    #[arg(long, default_value = "7.5.0")]
    version: String,

    /// Open messages in a browser window. Specify to choose an app or leave empty to use the
    /// default.
    #[arg(long, short)]
    browser: Option<String>,

    /// Where to store the html files. Defaults to the temp dir of the OS
    /// In linux this would generate two files:
    /// `/tmp/proton_htmls/[subject]/before.html` - The raw message
    /// `/tmp/proton_htmls/[subject]/after.html`  - The file after transforming
    #[arg(long)]
    html_dir: Option<PathBuf>,

    #[arg(long, short)]
    username: Option<String>,

    #[arg(long, short)]
    password: Option<String>,

    #[arg(long)]
    event_loop_time: Option<u64>,

    #[arg(long, default_value = "false")]
    trace_logs: bool,

    #[arg(long, default_value = "true")]
    use_emoji: bool,
}

impl CliArgs {
    pub fn dir(&self) -> &str {
        self.environment.as_deref().unwrap_or_default()
    }

    pub fn api_config(&self) -> ApiConfig {
        let env_id = if let Some(env) = &self.environment {
            EnvId::new_atlas_name(env)
        } else {
            EnvId::new_prod()
        };

        ApiConfig {
            app_details: self.app_details(),
            env_id,
            user_agent: None,
            proxy: None,
            resolver: None,
        }
    }

    pub fn app_details(&self) -> AppDetails {
        AppDetails {
            platform: self.platform.clone(),
            product: self.product.clone(),
            version: self.version.clone(),
        }
    }
}

static CLI_ARGS: LazyLock<CliArgs> = LazyLock::new(CliArgs::parse);

fn main() -> anyhow::Result<()> {
    // Trigger the global cli args message once so we can actually read the help string
    // correctly.
    let _ = &*CLI_ARGS;

    initialize_panic_handler();

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let state = rt.block_on(AppModel::new())?;
    let mut app = App::new(state);

    // Main loop happens here.
    let result = app.run(terminal, &rt);

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}
