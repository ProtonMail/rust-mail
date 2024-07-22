use crate::app::Command;
use crate::app_model::mailbox::BackgroundSender;
use crate::app_model::{mailbox, AppState, AppStateHandler};
use crate::messages::Messages;
use crate::widgets::CenteredThrobber;
use anyhow::anyhow;
use crossterm::event::Event;
use proton_mail_common::exports::tracing;
use proton_mail_common::proton_api_mail::domain::LabelId;
use proton_mail_common::{
    MailContext, MailContextError, MailUserContext, MailUserContextInitializationCallback,
    MailUserContextLoadingStage,
};
use ratatui::prelude::*;
use throbber_widgets_tui::ThrobberState;

pub enum Message {
    Init,
    InitComplete,
    InitFailed(MailContextError),
}
pub struct Model {
    ctx: MailUserContext,
    throbber_state: ThrobberState,
}

impl Model {
    pub fn new(ctx: MailUserContext) -> Self {
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

    fn update(
        &mut self,
        _: &MailContext,
        message: Messages,
        _: &BackgroundSender,
    ) -> Command<Messages> {
        let Messages::ContextInit(message) = message else {
            return Command::None;
        };

        match message {
            Message::Init => {
                let user_ctx = self.ctx.clone();
                Command::task(async move {
                    tracing::info!("Initializing user account");
                    let cb = InitCallback {};
                    let msg = if let Err((stage, e)) = user_ctx
                        .initialize_async(LabelId::inbox().clone(), &cb)
                        .await
                    {
                        tracing::error!("Failed to initialize account ({:?}): {e}", stage);
                        Message::InitFailed(e).into()
                    } else {
                        Message::InitComplete.into()
                    };

                    Command::message(msg)
                })
            }
            Message::InitComplete => match mailbox::Model::new(self.ctx.clone()) {
                Ok(model) => Command::message(Messages::SwitchAppState(model.into())),
                Err(e) => Command::message(e.into()),
            },
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

struct InitCallback {}

impl MailUserContextInitializationCallback for InitCallback {
    fn on_stage(&self, stage: MailUserContextLoadingStage) {
        tracing::info!("Initializing {:?}", stage);
    }

    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError) {
        tracing::error!("Failed to initialize account ({:?}): {err}", stage);
    }
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
