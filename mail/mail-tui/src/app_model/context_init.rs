use crate::app_model::{mailbox, AppState, AppStateHandler, BackgroundSender};
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
    fn on_state_enter(&mut self) -> Option<Messages> {
        Some(Message::Init.into())
    }
    fn handle_event(&mut self, _: Event) -> Option<Messages> {
        None
    }

    fn update(
        &mut self,
        ctx: &MailContext,
        message: Messages,
        sender: &BackgroundSender,
    ) -> Option<Messages> {
        let Messages::ContextInit(message) = message else {
            return None;
        };

        match message {
            Message::Init => {
                let user_ctx = self.ctx.clone();
                let sender = sender.clone();
                ctx.async_runtime().spawn(async move {
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

                    sender.send(msg);
                });
                None
            }
            Message::InitComplete => Some(match mailbox::Model::new(self.ctx.clone()) {
                Ok(model) => Messages::SwitchAppState(model.into()),
                Err(e) => e.into(),
            }),
            Message::InitFailed(e) => Some(Messages::DisplayError(None, anyhow!("{e}"))),
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
