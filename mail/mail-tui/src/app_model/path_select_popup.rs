use crate::app::Command;
use crate::app_model::Popup;
use crate::messages::Messages;
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui_explorer::FileExplorer;
use std::path::Path;

pub type PathSelectClosure = Box<dyn Fn(&Path) -> Command<Messages> + Send + 'static>;
/// Popup to select a filesystem path from your system.
pub struct PathSelectPopup {
    on_select: PathSelectClosure,
    state: FileExplorer,
}

impl PathSelectPopup {
    pub fn new(on_select: PathSelectClosure) -> Self {
        Self {
            on_select,
            state: FileExplorer::new().unwrap(),
        }
    }
}

impl Popup for PathSelectPopup {
    fn title(&self) -> Option<String> {
        Some("File Select".to_string())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Event::Key(KeyEvent { code, .. }) = &event {
            match code {
                KeyCode::Enter => {
                    if !self.state.current().is_dir() {
                        return Command::batch([
                            Command::message(Messages::DismissPopup),
                            (self.on_select)(self.state.current().path()),
                        ]);
                    }
                }
                KeyCode::Esc => {
                    return Command::message(Messages::DismissPopup);
                }
                _ => {}
            }
        }

        if let Err(e) = self.state.handle(&event) {
            return Command::message(anyhow!(e));
        }

        Command::None
    }
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(&self.state.widget(), area);
    }
}
