mod app;
mod events;
mod keychain;
mod queue;
mod state;
mod style;
mod tui_utils;
mod view;
mod views;
mod widgets;

use crate::app::App;
use crate::state::AppState;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{stdout, Stdout};

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

pub fn initialize_panic_handler() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        crossterm::execute!(std::io::stderr(), LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_panic_handler();

    let state = AppState::new()?;
    let mut app = App::new(state);
    app.push_view(views::SessionsView::new());
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;
    let result = app.run(terminal);
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}
