mod app;
// mod events;
mod app_model;
mod keychain;
mod messages;
mod widgets;

use crate::app::App;

use crate::app_model::AppModel;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{stdout, Stdout};

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

fn initialize_panic_handler() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        crossterm::execute!(std::io::stderr(), LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_panic_handler();

    let state = AppModel::new().await?;
    let mut app = App::new(state);
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    // TODO: use async commands to perform async queries
    #[allow(clippy::large_futures)]
    let result = app.run(terminal).await;
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}
