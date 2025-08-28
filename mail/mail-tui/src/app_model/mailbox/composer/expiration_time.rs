use crate::app::Command;
use crate::app_model::Popup;
use crate::app_model::mailbox::ComposerMessage;
use crate::messages::Messages;
use crate::widgets::utils::parse_date_time;
use crate::widgets::{TextInput, TextInputState};
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Margin;
use ratatui::style::Stylize;
use ratatui::text::Text;

pub struct ExpirationTimePopup {
    text_input_state: TextInputState,
    error: Option<String>,
}

impl ExpirationTimePopup {
    fn new() -> Self {
        Self {
            text_input_state: TextInputState::new().selected(true),
            error: None,
        }
    }
    pub fn open() -> Command<Messages> {
        Command::message(Messages::raise_popup(Self::new()))
    }
}

impl Popup for ExpirationTimePopup {
    fn title(&self) -> Option<String> {
        Some("Expiration Time".to_string())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        self.text_input_state.handle_event(&event);
        if let Event::Key(KeyEvent { code, .. }) = event
            && matches!(code, KeyCode::Enter)
        {
            match parse_date_time(self.text_input_state.value()) {
                Ok(date_time) => {
                    return Command::batch([
                        Command::message(Messages::DismissPopup),
                        Command::message(ComposerMessage::SetExpirationTime(date_time)),
                    ]);
                }
                Err(e) => {
                    self.error = Some(format!("Parse Error: {e}"));
                }
            }
        }
        Command::none()
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });

        let area = if let Some(error) = &self.error {
            let [_, error_area, _, remaining] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(4),
            ])
            .areas(area);
            frame.render_widget(
                Text::from(error.as_str()).centered().red().bold(),
                error_area,
            );
            remaining
        } else {
            area
        };

        let [help_area, _, text_input_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .areas(area);

        frame.render_widget(
            Text::from("Input Custom Date Time (format = DD/MM/YYYY HH:MM)").centered(),
            help_area,
        );
        frame.render_stateful_widget(
            TextInput::new("Custom Date"),
            text_input_area,
            &mut self.text_input_state,
        );
    }
}
