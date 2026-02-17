use crate::app::Command;
use crate::app_model::{AppState, AppStateHandler, context_init, login};
use crate::messages::Messages;
use crate::messages::Messages::DismissBackgroundProgress;
use crate::widgets::{TextInput, TextInputState};
use anyhow::anyhow;
use proton_account_api::login::{LoginError, LoginFlow};
use proton_mail_common::MailContext;
use ratatui::crossterm::event::{Event, KeyCode};
use ratatui::layout::Flex;
use ratatui::prelude::*;
use std::sync::Arc;

pub enum Message {
    Abort,
    Submit,
    Success(LoginFlow),
    Failed(LoginFlow, LoginError),
}

pub struct MboxPasswordModel {
    flow: Option<LoginFlow>,
    input_state: TextInputState,
}

impl MboxPasswordModel {
    pub fn new(flow: LoginFlow) -> Self {
        Self {
            flow: Some(flow),
            input_state: TextInputState::new().selected(true).secret(true),
        }
    }
}

impl AppStateHandler for MboxPasswordModel {
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(k) = event else {
            return Command::None;
        };
        match k.code {
            KeyCode::Esc => Command::message(Message::Abort),
            KeyCode::Enter => Command::message(Message::Submit),
            _ => {
                self.input_state.handle_event(&event);
                Command::None
            }
        }
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::MboxPassword(message) = message else {
            return Command::None;
        };

        match message {
            Message::Abort => {
                if let Some(_flow) = self.flow.take() {
                    //TODO: Logout
                }
                Command::message(Messages::SwitchAppState(login::LoginModel::new().into()))
            }
            Message::Submit => {
                if self.input_state.value().is_empty() {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow! {"Two factor code can not be empty"},
                    ));
                }

                let Some(mut flow) = self.flow.take() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Invalid State"),
                    ));
                };

                let pass = self.input_state.value().to_owned();
                Command::batch([
                    Command::Message(Messages::DisplayBackgroundProgress(
                        "Unlocking mailbox ...".to_owned(),
                    )),
                    Command::task(async move {
                        let message = if let Err(e) = flow.submit_mailbox_password(pass).await {
                            Message::Failed(flow, e)
                        } else {
                            Message::Success(flow)
                        };

                        Command::batch([
                            Command::message(message),
                            Command::message(DismissBackgroundProgress),
                        ])
                    }),
                ])
            }
            Message::Success(mut flow) => {
                if flow.is_logged_in() {
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
                } else {
                    Command::message(Messages::DisplayError(None, anyhow!("Invalid State")))
                }
            }
            Message::Failed(flow, err) => {
                self.flow = Some(flow);
                Command::message(Messages::DisplayError(None, anyhow!("{err}")))
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            horizontal: 10,
            vertical: 2,
        });

        let [_, email_area, _] = Layout::default()
            .direction(Direction::Vertical)
            .flex(Flex::Center)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .areas(area);

        frame.render_stateful_widget(
            TextInput::new("Mbox Password:").with_max_label_length(15),
            email_area,
            &mut self.input_state,
        );

        let (x, y) = self.input_state.frame_cursor();
        frame.set_cursor_position(Position { x, y });
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Line::from(vec![
                Span::from("Esc: ").bold(),
                Span::from("Cancel"),
                Span::from(" Enter: ").bold(),
                Span::from("Submit"),
            ]),
            area,
        );
    }

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        [("esc", "Cancel"), ("Enter", "Submit")].to_vec()
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("TwoFA"), area);
    }
}

impl From<MboxPasswordModel> for AppState {
    fn from(value: MboxPasswordModel) -> Self {
        Self::MboxPassowrd(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::MboxPassword(value)
    }
}
