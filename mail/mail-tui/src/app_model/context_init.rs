use crate::app::Command;
use crate::app_model::{AppState, AppStateHandler, mailbox};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use proton_account_api::login::LoginFlow;
use proton_mail_common::{MailContext, MailContextError, MailUserContext};
use ratatui::crossterm::event::Event;
use ratatui::prelude::*;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

pub enum Message {
    Init,
    InitComplete(Arc<MailUserContext>),
    InitFailed(MailContextError),
}
pub struct ContextInitModel {
    flow: Option<LoginFlow>,
    throbber_state: ThrobberState,
}

impl ContextInitModel {
    pub fn new(flow: LoginFlow) -> Self {
        Self {
            flow: Some(flow),
            throbber_state: ThrobberState::default(),
        }
    }
}

impl AppStateHandler for ContextInitModel {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init)
    }
    fn handle_event(&mut self, _: Event) -> Command<Messages> {
        Command::none()
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::ContextInit(message) = message else {
            return Command::None;
        };

        match message {
            Message::Init => {
                let Some(mut flow) = self.flow.take() else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("No login flow"),
                    ));
                };
                if !flow.is_logged_in() {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Login flow has invalid state"),
                    ));
                }

                let ctx = ctx.clone();
                Command::task(async move {
                    tracing::info!("Initializing user account");
                    let msg = match ctx.user_context_from_login_flow(&mut flow).await {
                        Ok(ctx) => Message::InitComplete(ctx),
                        Err(e) => {
                            tracing::error!("Failed to initialize account {e:?}");
                            Message::InitFailed(e)
                        }
                    };

                    Command::message(msg)
                })
            }
            Message::InitComplete(ctx) => Command::task(async move {
                match mailbox::MailboxModel::new(ctx).await {
                    Ok(model) => Command::message(Messages::SwitchAppState(model.into())),
                    Err(e) => Command::message(e),
                }
            }),
            Message::InitFailed(e) => {
                Command::message(Messages::DisplayError(None, anyhow!("{e}")))
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let throbber = throbber_widgets_tui::Throbber::default()
            .label("Initializing user...")
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);
        frame.render_stateful_widget(
            CenteredThrobber::new(throbber),
            area,
            &mut self.throbber_state,
        );
    }

    fn view_help_bar(&mut self, _: &mut Frame, _: Rect) {}

    fn view_status_bar(&mut self, _: &mut Frame, _: Rect) {}
    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        vec![]
    }
}

impl From<ContextInitModel> for AppState {
    fn from(value: ContextInitModel) -> Self {
        Self::ContextInit(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::ContextInit(value)
    }
}
