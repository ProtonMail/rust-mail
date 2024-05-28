use crate::app_model::{context_init, login, AppState, AppStateHandler, BackgroundSender};
use crate::messages::Messages;
use crate::widgets::{TextInput, TextInputState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::proton_api_core::login::{Error, Flow};
use proton_mail_common::MailContext;
use ratatui::layout::Flex;
use ratatui::prelude::*;

pub enum Message {
    Abort,
    Submit,
    TwoFASuccess(Flow),
    TwoFAFailed(Flow, Error),
}

pub struct Model {
    flow: Option<Flow>,
    input_state: TextInputState,
}

impl Model {
    pub fn new(flow: Flow) -> Self {
        Self {
            flow: Some(flow),
            input_state: TextInputState::new().selected(true),
        }
    }
}

impl AppStateHandler for Model {
    fn handle_event(&mut self, event: Event) -> Option<Messages> {
        let Event::Key(k) = event else {
            return None;
        };
        match k.code {
            KeyCode::Esc => Some(Message::Abort.into()),
            KeyCode::Enter => Some(Message::Submit.into()),
            _ => {
                self.input_state.handle_event(&event);
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
        let Messages::TwoFA(message) = message else {
            return None;
        };

        match message {
            Message::Abort => {
                if let Some(_flow) = self.flow.take() {
                    //TODO: Logout
                }
                Some(Messages::SwitchAppState(login::Model::new().into()))
            }
            Message::Submit => {
                if self.input_state.value().is_empty() {
                    return Some(Messages::DisplayError(
                        None,
                        anyhow! {"Two factor code can not be empty"},
                    ));
                }

                let Some(mut flow) = self.flow.take() else {
                    return Some(Messages::DisplayError(None, anyhow!("Invalid State")));
                };

                let sender = sender.clone();
                let code = self.input_state.value().to_owned();
                ctx.async_runtime().spawn(async move {
                    scopeguard::defer! {
                        sender.send(Messages::DismissBackgroundProgress);
                    }
                    if let Err(e) = flow.submit_totp(&code).await {
                        sender.send(Message::TwoFAFailed(flow, e).into());
                    } else {
                        sender.send(Message::TwoFASuccess(flow).into());
                    }
                });

                Some(Messages::DisplayBackgroundProgress(
                    "Submitting Two Factor Code ...".to_owned(),
                ))
            }
            Message::TwoFASuccess(flow) => {
                if flow.is_logged_in() {
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
                } else {
                    Some(Messages::DisplayError(None, anyhow!("Invalid State")))
                }
            }
            Message::TwoFAFailed(flow, err) => {
                self.flow = Some(flow);
                Some(Messages::DisplayError(None, anyhow!("{err}")))
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(&Margin {
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
            TextInput::new("Two Factor Code:").with_max_label_length(15),
            email_area,
            &mut self.input_state,
        );

        let (x, y) = self.input_state.frame_cursor();
        frame.set_cursor(x, y);
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

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Text::from("TwoFA"), area);
    }
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::TwoFA(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::TwoFA(value)
    }
}
