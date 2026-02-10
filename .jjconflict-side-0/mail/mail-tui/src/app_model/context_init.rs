use crate::app::Command;
use crate::app_model::{AppState, AppStateHandler, mailbox};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, NewMailUserContextOptions,
};
use ratatui::crossterm::event::Event;
use ratatui::prelude::*;
use std::sync::Arc;
use throbber_widgets_tui::ThrobberState;

pub enum Message {
    Init,
    InitComplete,
    InitFailed(MailContextError),
}
pub struct ContextInitModel {
    ctx: Arc<MailUserContext>,
    throbber_state: ThrobberState,
}

impl ContextInitModel {
    pub fn new(ctx: Arc<MailUserContext>) -> Self {
        Self {
            ctx,
            throbber_state: ThrobberState::default(),
        }
    }

    pub fn ctx(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.ctx)
    }
}

impl AppStateHandler for ContextInitModel {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Command::message(Message::Init)
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
                    let msg = if let Err(e) = MailUserContext::initialize_async(
                        user_ctx,
                        NewMailUserContextOptions::default(),
                    )
                    .await
                    {
                        tracing::error!("Failed to initialize account {e:?}");
                        Message::InitFailed(e)
                    } else {
                        Message::InitComplete
                    };

                    Command::message(msg)
                })
            }
            Message::InitComplete => {
                let ctx = Arc::clone(&self.ctx);
                Command::task(async move {
                    match mailbox::MailboxModel::new(ctx).await {
                        Ok(model) => Command::message(Messages::SwitchAppState(model.into())),
                        Err(e) => Command::message(e),
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
