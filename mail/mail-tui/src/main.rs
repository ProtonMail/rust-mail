// #![feature(try_blocks)]
// #![feature(try_trait_v2)]
mod app;
// mod events;
mod app_model;
mod keychain;
mod messages;
mod widgets;

use crate::app::App;
use app_model::env::Env;
use clap::Parser;

use crate::app_model::AppModel;
use app_model::AppConfig;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{stdout, Stdout};
use tokio::runtime::Runtime;

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

fn initialize_panic_handler() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        crossterm::execute!(std::io::stderr(), LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}

#[derive(Parser)]
#[command(name = "proton-mail-tui", about = "proton mail tui application")]
struct Cli {
    /// Enable proton black environment
    #[arg(long)]
    dev: bool,
}

impl From<Cli> for AppConfig {
    fn from(value: Cli) -> Self {
        let env = if value.dev { Env::Dev } else { Env::Prod };
        AppConfig { env }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_panic_handler();
    let runtime = Runtime::new()?;
    let args = Cli::parse();
    let state = AppModel::new(&runtime, &AppConfig::from(args))?;
    let mut app = App::new(runtime, state);
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    let result = app.run(terminal);
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}
