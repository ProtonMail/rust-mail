use crate::app::Command;
use crate::app_model::{AppState, AppStateHandler, mailbox};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use proton_mail_common::{MailContext, MailContextError, MailUserContext};
use ratatui::crossterm::event::Event;
use ratatui::prelude::*;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

pub enum Message {
    Init,
    InitComplete,
    InitFailed(MailContextError),
}
pub struct Model {
    ctx: Arc<MailUserContext>,
    throbber_state: ThrobberState,
}

impl Model {
    pub fn new(ctx: Arc<MailUserContext>) -> Self {
        Self {
            ctx,
            throbber_state: ThrobberState::default(),
        }
    }
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init.into())
    }
    fn handle_event(&mut self, _: Event) -> Command<Messages> {
        Command::none()
    }

    fn update(&mut self, _: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::ContextInit(message) = message else {
            return Command::None;
        };

        match message {
            Message::Init => {
                let user_ctx = self.ctx.clone();
                Command::task(async move {
                    tracing::info!("Initializing user account");
                    let msg = if let Err(e) = MailUserContext::initialize_async(user_ctx).await {
                        tracing::error!("Failed to initialize account {e:?}");
                        Message::InitFailed(e).into()
                    } else {
                        Message::InitComplete.into()
                    };

                    Command::message(msg)
                })
            }
            Message::InitComplete => {
                let ctx = Arc::clone(&self.ctx);
                Command::task(async move {
                    match mailbox::Model::new(ctx).await {
                        Ok(model) => Command::message(Messages::SwitchAppState(model.into())),
                        Err(e) => Command::message(e.into()),
                    }
                })
            }
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
}

impl From<Model> for AppState {
    fn from(value: Model) -> Self {
        Self::ContextInit(value)
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::ContextInit(value)
    }
}
