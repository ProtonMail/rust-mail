//! Simulation of background work

use crate::app::Command;
use crate::app_model::AppStateHandler;
use crate::messages::Messages;
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use futures::FutureExt;
use proton_mail_common::background_execution::{
    BackgroundExecutionContext, BackgroundExecutionStatus,
};
use proton_mail_common::{MailContext, MailContextResult, MailUserContext};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Line, Span, Stylize};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub type OnClose = Box<dyn Fn(Arc<MailUserContext>) -> Command<Messages> + Send + 'static>;
pub struct Model {
    user_context: Arc<MailUserContext>,
    on_close: OnClose,
    background_execution_state: BackgroundExecutionState,
}

impl Model {
    pub fn new(ctx: Arc<MailUserContext>, on_close: OnClose) -> Self {
        Self {
            user_context: ctx,
            on_close,
            background_execution_state: BackgroundExecutionState::Stopped,
        }
    }

    fn start_background_execution(&mut self, ctx: Arc<MailContext>) -> Command<Messages> {
        if matches!(
            self.background_execution_state,
            BackgroundExecutionState::Running(_)
        ) {
            return Command::message(Messages::DisplayError(
                None,
                anyhow!("Background executor already running"),
            ));
        }
        let bg_executor = match BackgroundExecutionContext::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                return Command::message(Messages::DisplayError(
                    None,
                    anyhow!("Failed to create background executor: {e}"),
                ));
            }
        };

        let cancellation_token = CancellationToken::new();
        let cancellation_token_cloned = cancellation_token.clone();
        self.background_execution_state = BackgroundExecutionState::Running(cancellation_token);
        Command::background_task(move |sender| {
            async move {
                let r = bg_executor
                    .run(
                        &ctx,
                        async {
                            cancellation_token_cloned.cancelled().await;
                            true
                        },
                        Duration::from_secs(30),
                    )
                    .await;
                let _ = sender
                    .send_async(Message::BackgroundExecutionFinished(r).into())
                    .await;
            }
            .boxed()
        })
    }

    fn stop_background_execution(&mut self) -> Command<Messages> {
        let BackgroundExecutionState::Running(cancellation_token) = std::mem::replace(
            &mut self.background_execution_state,
            BackgroundExecutionState::Stopped,
        ) else {
            return Command::message(Messages::DisplayError(
                None,
                anyhow!("Background executor not running"),
            ));
        };
        cancellation_token.cancel();
        Command::none()
    }

    fn on_background_execution_finished(
        &mut self,
        result: MailContextResult<BackgroundExecutionStatus>,
    ) -> Command<Messages> {
        self.background_execution_state = BackgroundExecutionState::Stopped;
        match result {
            Ok(status) => Command::message(Messages::DisplayInfo(
                None,
                format!("Background Execution finished with status: {status:?}"),
            )),
            Err(e) => Command::message(Messages::DisplayError(
                None,
                anyhow!("Background execution failed: {e:?}"),
            )),
        }
    }
}

pub enum Message {
    Init,
    StartBackgroundWorker,
    StopBackgroundWorker,
    BackgroundExecutionFinished(MailContextResult<BackgroundExecutionStatus>),
    Exit,
}

impl From<Message> for Command<Messages> {
    fn from(msg: Message) -> Self {
        Command::message(Messages::BackgroundWorker(msg))
    }
}

#[derive(Debug)]
enum BackgroundExecutionState {
    Stopped,
    Running(CancellationToken),
}

impl AppStateHandler for Model {
    fn on_state_enter(&mut self) -> Command<Messages> {
        Message::Init.into()
    }
    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::none();
        };
        match key.code {
            KeyCode::Char('s') => Message::StartBackgroundWorker.into(),
            KeyCode::Char('t') => Message::StopBackgroundWorker.into(),
            KeyCode::Char('q') => Message::Exit.into(),
            _ => Command::none(),
        }
    }

    fn update(&mut self, ctx: &Arc<MailContext>, message: Messages) -> Command<Messages> {
        let Messages::BackgroundWorker(message) = message else {
            return Command::none();
        };

        match message {
            Message::Init => {
                ctx.core_context().task_service().pause_main();
                Command::none()
            }
            Message::StartBackgroundWorker => self.start_background_execution(ctx.clone()),
            Message::StopBackgroundWorker => self.stop_background_execution(),
            Message::Exit => {
                if let BackgroundExecutionState::Running(cancellation_token) =
                    &mut self.background_execution_state
                {
                    cancellation_token.cancel();
                }
                let ctx = ctx.clone();
                ctx.core_context().task_service().resume_main();
                (self.on_close)(self.user_context.clone())
            }
            Message::BackgroundExecutionFinished(r) => self.on_background_execution_finished(r),
        }
    }

    fn view(&mut self, _frame: &mut Frame, _area: Rect) {
        //TODO: put some useful stats here?
    }

    fn view_help_bar(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Line::from(vec![
                Span::from("s: ").bold(),
                Span::from("Start Background Worker "),
                Span::from("t: ").bold(),
                Span::from("Stop Background Worker "),
                Span::from("q: ").bold(),
                Span::from("Exit"),
            ]),
            area,
        );
    }

    fn view_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        match &self.background_execution_state {
            BackgroundExecutionState::Stopped => {
                frame.render_widget(Line::from("Background Execution: Stopped"), area);
            }
            BackgroundExecutionState::Running(_) => {
                frame.render_widget(Line::from("Background Execution: Running"), area);
            }
        }
    }
}
