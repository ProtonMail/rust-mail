use crate::app_model::{context_init, twofa, AppState, AppStateHandler, BackgroundSender};
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::proton_api_core::login::{Error, Flow};
use proton_mail_common::MailContext;
use ratatui::layout::Flex;
use ratatui::prelude::*;
use secrecy::{ExposeSecret, SecretString};

pub enum Message {
    Submit,
    ToggleInput,
    LoginSuccess(Flow),
    LoginFailed(Error),
}

pub struct Model {
    email_input_state: TextInputState,
    password_input_state: TextInputState,
    active_input: ActiveInput,
}

impl Model {
    pub fn new() -> Self {
        Self {
            email_input_state: TextInputState::new().selected(true),
            password_input_state: TextInputState::new().secret(true),
            active_input: ActiveInput::Email,
        }
    }

    fn active_text_input_state(&self) -> &TextInputState {
        match self.active_input {
            ActiveInput::Email => &self.email_input_state,
            ActiveInput::Password => &self.password_input_state,
        }
    }

    fn active_text_input_state_mut(&mut self) -> &mut TextInputState {
        match self.active_input {
            ActiveInput::Email => &mut self.email_input_state,
            ActiveInput::Password => &mut self.password_input_state,
        }
    }
}

impl AppStateHandler for Model {
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        let Event::Key(k) = event else {
            return None;
        };
        match k.code {
            KeyCode::Esc => {
                //TODO: return to previous state?
                None
            }
            KeyCode::Enter => Some(Message::Submit.into()),
            KeyCode::Tab => Some(Message::ToggleInput.into()),
            _ => {
                self.active_text_input_state_mut().handle_event(&event);
                None
            }
        }
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Messages,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        let Messages::Login(message) = message else {
            return None;
        };

        match message {
            Message::ToggleInput => {
                self.active_text_input_state_mut().set_selected(false);
                self.active_input = match self.active_input {
                    ActiveInput::Email => ActiveInput::Password,
                    ActiveInput::Password => ActiveInput::Email,
                };
                self.active_text_input_state_mut().set_selected(true);
                None
            }
            Message::Submit => {
                if self.password_input_state.value().is_empty()
                    || self.email_input_state.value().is_empty()
                {
                    return Some(Messages::DisplayError(
                        None,
                        anyhow! {"Email and Password can not be empty"},
                    ));
                }

                let mut flow = match ctx.new_login_flow(None) {
                    Ok(f) => f,
                    Err(e) => {
                        return Some(e.into());
                    }
                };

                let sender = sender.clone();
                let email = self.email_input_state.value().trim().to_owned();
                let password = SecretString::new(self.password_input_state.value().to_owned());
                ctx.async_runtime().spawn(async move {
                    scopeguard::defer! {
                        sender.send(Messages::DismissBackgroundProgress);
                    }
                    if let Err(e) = flow
                        .login(email.as_str(), password.expose_secret().as_str(), None)
                        .await
                    {
                        sender.send(Message::LoginFailed(e).into());
                    } else {
                        sender.send(Message::LoginSuccess(flow).into());
                    }
                });

                Some(Messages::DisplayBackgroundProgress(
                    "Performing Login ...".to_owned(),
                ))
            }
            Message::LoginSuccess(flow) => {
                if flow.is_awaiting_2fa() {
                    Some(Messages::SwitchAppState(twofa::Model::new(flow).into()))
                } else {
                    match ctx.user_context_from_login_flow(&flow) {
                        Ok(context) => Some(Messages::SwitchAppState(
                            context_init::Model::new(context).into(),
                        )),
                        Err(e) => {
                            let e = anyhow!("Failed to login: {e}");
                            tracing::error!("{e}");
                            Some(Messages::DisplayError(None, e))
                        }
                    }
                }
            }
            Message::LoginFailed(err) => Some(Messages::DisplayError(None, anyhow!("{err}"))),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            horizontal: 10,
            vertical: 2,
        });

        let [_, email_area, password_area, _] = Layout::default()
            .direction(Direction::Vertical)
            .flex(Flex::Center)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .areas(area);

        let max_label_size: u16 = 10;
        frame.render_stateful_widget(
            TextInput::new("Email:").with_max_label_length(max_label_size),
            email_area,
            &mut self.email_input_state,
        );

        frame.render_stateful_widget(
            TextInput::new("Password:").with_max_label_length(max_label_size),
            password_area,
            &mut self.password_input_state,
        );

        let (x, y) = self.active_text_input_state().frame_cursor();
        frame.set_cursor(x, y);
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Line::from(vec![
                Span::styled("Enter: ", Style::new().bold()),
                Span::raw("Submit"),
                Span::styled(" Tab: ", Style::new().bold()),
                Span::raw("Switch Input"),
            ]),
            area,
        );
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("Login"), area);
    }
}

enum ActiveInput {
    Email,
    Password,
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::Login(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Login(value)
    }
}
