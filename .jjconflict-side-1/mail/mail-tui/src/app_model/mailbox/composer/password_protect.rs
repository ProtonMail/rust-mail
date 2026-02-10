use crate::app::Command;
use crate::app_model::Popup;
use crate::app_model::mailbox::ComposerMessage;
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use secrecy::SecretString;

pub struct PasswordProtectPopup {
    password_text_input_state: TextInputState,
    hint_text_input_state: TextInputState,
    selected: Selected,
}

enum Selected {
    Password,
    Hint,
}

impl PasswordProtectPopup {
    fn new() -> Self {
        Self {
            password_text_input_state: TextInputState::new().selected(true).secret(true),
            hint_text_input_state: TextInputState::new(),
            selected: Selected::Password,
        }
    }

    pub fn open() -> Command<Messages> {
        Command::message(Messages::raise_popup(Self::new()))
    }
}

impl Popup for PasswordProtectPopup {
    fn title(&self) -> Option<String> {
        Some("Password Protect Email".to_owned())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        if let Event::Key(event) = &event {
            match event.code {
                KeyCode::Tab => match self.selected {
                    Selected::Hint => {
                        self.selected = Selected::Password;
                        self.password_text_input_state.set_selected(true);
                        self.hint_text_input_state.set_selected(false);
                    }
                    Selected::Password => {
                        self.selected = Selected::Hint;
                        self.password_text_input_state.set_selected(false);
                        self.hint_text_input_state.set_selected(true);
                    }
                },
                KeyCode::Enter => {
                    let cmd = if self.password_text_input_state.value().is_empty() {
                        Command::message(Messages::DisplayError(None, anyhow!("Password is empty")))
                    } else {
                        Command::message(ComposerMessage::SetPasswordProtection(
                            SecretString::new(String::from(self.password_text_input_state.value())),
                            if self.hint_text_input_state.value().is_empty() {
                                None
                            } else {
                                Some(self.hint_text_input_state.value().to_owned())
                            },
                        ))
                    };

                    return Command::batch([Command::message(Messages::DismissPopup), cmd]);
                }
                _ => {}
            }
        }

        match self.selected {
            Selected::Password => {
                self.password_text_input_state.handle_event(&event);
            }
            Selected::Hint => {
                self.hint_text_input_state.handle_event(&event);
            }
        }
        Command::none()
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin::new(1, 1));
        let [password_area, _, hint_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .areas(area);
        frame.render_stateful_widget(
            TextInput::new("Password"),
            password_area,
            &mut self.password_text_input_state,
        );
        frame.render_stateful_widget(
            TextInput::new("Hint"),
            hint_area,
            &mut self.hint_text_input_state,
        );
    }

    fn height(&self) -> Constraint {
        Constraint::Length(12)
    }

    fn width(&self) -> Constraint {
        Constraint::Length(60)
    }
}
