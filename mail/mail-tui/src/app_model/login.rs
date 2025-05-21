use crate::CLI_ARGS;
use crate::app::Command;
use crate::app_model::{AppState, AppStateHandler, context_init, twofa};
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use anyhow::anyhow;
use proton_account_api::login::{LoginError, LoginFlow};
use proton_mail_common::MailContext;
use proton_mail_common::proton_mail_api::proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;

pub enum Message {
    Submit,
    ToggleInput,
    LoginSuccess(LoginFlow),
    LoginFailed(LoginError),
}

pub struct LoginModel {
    email_input_state: TextInputState,
    password_input_state: TextInputState,
    active_input: ActiveInput,
}

impl LoginModel {
    pub fn new() -> Self {
        let email = CLI_ARGS.username.as_deref().unwrap_or_default();
        let password = CLI_ARGS.password.as_deref().unwrap_or_default();
        Self {
            email_input_state: TextInputState::with_value(email).selected(true),
            password_input_state: TextInputState::with_value(password).secret(true),
            active_input: ActiveInput::Email,
        }
    }

    pub fn with_email(email: String) -> Self {
        Self {
            email_input_state: TextInputState::with_value(email).selected(true),
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

impl AppStateHandler for LoginModel {
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(k) = event else {
            return Command::None;
        };
        match k.code {
            KeyCode::Esc => {
                //TODO: return to previous state?
                Command::none()
            }
            KeyCode::Enter => Command::message(Message::Submit.into()),
            KeyCode::Tab => Command::message(Message::ToggleInput.into()),
            _ => {
                self.active_text_input_state_mut().handle_event(&event);
                Command::none()
            }
        }
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::Login(message) = message else {
            return Command::None;
        };

        match message {
            Message::ToggleInput => {
                self.active_text_input_state_mut().set_selected(false);
                self.active_input = match self.active_input {
                    ActiveInput::Email => ActiveInput::Password,
                    ActiveInput::Password => ActiveInput::Email,
                };
                self.active_text_input_state_mut().set_selected(true);
                Command::None
            }
            Message::Submit => {
                if self.password_input_state.value().is_empty()
                    || self.email_input_state.value().is_empty()
                {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow! {"Email and Password can not be empty"},
                    ));
                }

                let email = self.email_input_state.value().trim().to_owned();
                let password = SecretString::new(self.password_input_state.value().to_owned());
                let ctx = Arc::clone(ctx);
                Command::batch([
                    Command::message(Messages::DisplayBackgroundProgress(
                        "Performing Login ...".to_owned(),
                    )),
                    Command::task(async move {
                        let mut flow = match ctx.new_login_flow().await {
                            Ok(f) => f,
                            Err(e) => {
                                return Command::message(e.into());
                            }
                        };
                        let message = if let Err(e) = flow
                            .login(
                                email.clone(),
                                password.expose_secret().to_owned(),
                                LoginExtraInfo::default(),
                            )
                            .await
                        {
                            Message::LoginFailed(e).into()
                        } else {
                            Message::LoginSuccess(flow).into()
                        };
                        Command::batch([
                            Command::Message(Messages::DismissBackgroundProgress),
                            Command::message(message),
                        ])
                    }),
                ])
            }
            Message::LoginSuccess(mut flow) => {
                if flow.is_awaiting_2fa() {
                    Command::message(Messages::SwitchAppState(
                        twofa::TwoFaModel::new(flow).into(),
                    ))
                } else {
                    let ctx = Arc::clone(ctx);
                    Command::task(async move {
                        match ctx.user_context_from_login_flow(&mut flow).await {
                            Ok(context) => Command::message(Messages::SwitchAppState(
                                context_init::ContextInitModel::new(context).into(),
                            )),
                            Err(e) => {
                                let e = anyhow!("Failed to login: {e}");
                                tracing::error!("{e:?}");
                                Command::message(Messages::DisplayError(None, e))
                            }
                        }
                    })
                }
            }
            Message::LoginFailed(err) => {
                Command::message(Messages::DisplayError(None, anyhow!("{err}")))
            }
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
        frame.set_cursor_position(Position { x, y });
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

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        vec![("enter", "Submit"), ("tab", "Switch Input")]
    }
}

enum ActiveInput {
    Email,
    Password,
}

impl From<LoginModel> for AppState {
    fn from(value: LoginModel) -> Self {
        Self::Login(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Login(value)
    }
}
