//! Simulation of background work

use crate::app::Command;
use crate::app_model::AppStateHandler;
use crate::messages::Messages;
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use futures::FutureExt;
use proton_core_common::models::{ModelExtension, User};
use proton_mail_common::background_execution::{
    BackgroundExecutionContext, BackgroundExecutionResult, BackgroundExecutionStatus,
};
use proton_mail_common::{MailContext, MailContextResult, MailUserContext};
use ratatui::Frame;
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::prelude::{Line, Span, Stylize};
use ratatui::widgets::{Cell, Row, Table};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub type OnClose = Box<dyn Fn(Arc<MailUserContext>) -> Command<Messages> + Send + 'static>;
pub struct BackgroundModel {
    user_context: Arc<MailUserContext>,
    on_close: OnClose,
    background_execution_state: BackgroundExecutionState,
    last_execution_status: Option<BackgroundExecutionStatus>,
    stats: Option<BackgroundExecutionStats>,
}

impl BackgroundModel {
    pub fn new(ctx: Arc<MailUserContext>, on_close: OnClose) -> Self {
        Self {
            user_context: ctx,
            on_close,
            background_execution_state: BackgroundExecutionState::Stopped,
            last_execution_status: None,
            stats: None,
        }
    }

    pub fn ctx(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.user_context)
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
        let ctx_cloned = ctx.clone();
        self.background_execution_state = BackgroundExecutionState::Running(cancellation_token);
        Command::batch([
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
            }),
            self.update_stats(ctx_cloned),
        ])
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
        result: MailContextResult<BackgroundExecutionResult>,
    ) -> Command<Messages> {
        self.background_execution_state = BackgroundExecutionState::Stopped;
        match result {
            Ok(result) => {
                self.last_execution_status = Some(result.status);
                if let Some(stats) = self.stats.as_mut() {
                    stats.has_unsent_messages = result.has_unsent_messages;
                }
                Command::none()
            }
            Err(e) => Command::message(Messages::DisplayError(
                None,
                anyhow!("Background execution failed: {e:?}"),
            )),
        }
    }

    fn update_stats(&self, ctx: Arc<MailContext>) -> Command<Messages> {
        if let BackgroundExecutionState::Stopped = &self.background_execution_state {
            return Command::none();
        }
        Command::task(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            tracing::info!("Updating background stats");
            let user_ctxs = match ctx.get_all_logged_in_and_initialized_user_contexts().await {
                Ok(ctxs) => ctxs,
                Err(e) => {
                    return Command::Message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to get logged in user ctxs: {e}"),
                    ));
                }
            };

            let mut user_stats = BTreeMap::new();
            let mut has_unsent_messages = false;
            for user_ctx in user_ctxs {
                let Ok(tether) = user_ctx.user_stash().connection().await else {
                    return Command::message(Messages::DisplayError(
                        None,
                        anyhow!("Failed to acquire db connection"),
                    ));
                };
                let user = match User::find_by_id(user_ctx.user_id().clone(), &tether).await {
                    Ok(user) => user,
                    Err(e) => {
                        return Command::Message(Messages::DisplayError(
                            None,
                            anyhow!("Failed to load user {}: {e:?}", user_ctx.user_id()),
                        ));
                    }
                };

                let Some(user) = user else {
                    tracing::warn!("Could not find user with id = {}", user_ctx.user_id());
                    continue;
                };
                let pending_count = match user_ctx.action_queue().queued_actions_count().await {
                    Ok(c) => c,
                    Err(e) => {
                        return Command::Message(Messages::DisplayError(
                            None,
                            anyhow!(
                                "Failed to pending action count for user {}: {e:?}",
                                user_ctx.user_id()
                            ),
                        ));
                    }
                };
                let unsent_message_count = match ctx
                    .get_unsent_messages_ids_for_user(user_ctx.user_id().clone())
                    .await
                {
                    Ok(c) => c.len(),
                    Err(e) => {
                        return Command::Message(Messages::DisplayError(
                            None,
                            anyhow!(
                                "Failed to unsent message count for user {}: {e:?}",
                                user_ctx.user_id()
                            ),
                        ));
                    }
                };

                has_unsent_messages =
                    has_unsent_messages || user_ctx.has_unsent_messages().await.unwrap_or(false);

                user_stats.insert(
                    user.email,
                    BackgroundExecutionUserStats {
                        pending_action_count: pending_count,
                        unsent_message_count,
                    },
                );
            }

            Message::BackgroundStatsRefreshed(BackgroundExecutionStats {
                has_unsent_messages,
                user_stats,
            })
            .into()
        })
    }
    fn on_background_stats_update(
        &mut self,
        ctx: Arc<MailContext>,
        background_execution_stats: BackgroundExecutionStats,
    ) -> Command<Messages> {
        self.stats = Some(background_execution_stats);
        self.update_stats(ctx)
    }
}

pub enum Message {
    Init,
    StartBackgroundWorker,
    StopBackgroundWorker,
    BackgroundExecutionFinished(MailContextResult<BackgroundExecutionResult>),
    BackgroundStatsRefreshed(BackgroundExecutionStats),
    Exit,
}

/// Collection of background execution info which we can track.
pub struct BackgroundExecutionStats {
    has_unsent_messages: bool,
    /// User statistics by email address
    user_stats: BTreeMap<String, BackgroundExecutionUserStats>,
}

/// Collection of background execution info which we can track per user.
pub struct BackgroundExecutionUserStats {
    pending_action_count: u64,
    unsent_message_count: usize,
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

impl AppStateHandler for BackgroundModel {
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
            Message::BackgroundStatsRefreshed(state) => {
                self.on_background_stats_update(ctx.clone(), state)
            }
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let mut rows = Vec::with_capacity(8);

        rows.push(Row::new([
            Cell::from(Span::from("Last Execution Status: ").bold()),
            Cell::from(
                self.last_execution_status
                    .map_or(String::from("None"), |s| format!("{s:?}")),
            ),
        ]));

        if let Some(stats) = &self.stats {
            rows.push(Row::new([
                Cell::from(Span::from("Unsent Messages: ").bold()),
                Cell::from(stats.has_unsent_messages.to_string()),
            ]));

            for (user_id, user_stats) in &stats.user_stats {
                rows.push(Row::new([Cell::from(""), Cell::from("")]));
                rows.push(Row::new([
                    Cell::from(Span::from(user_id.clone())).bold(),
                    Cell::from(""),
                ]));
                rows.push(Row::new([
                    Cell::from("Pending Actions: "),
                    Cell::from(user_stats.pending_action_count.to_string()),
                ]));
                rows.push(Row::new([
                    Cell::from("Unsent Messages: "),
                    Cell::from(user_stats.unsent_message_count.to_string()),
                ]));
            }
        }

        let widths = [Constraint::Length(40), Constraint::Min(1)];
        let table = Table::new(rows, widths).column_spacing(1);
        frame.render_widget(
            table,
            area.inner(Margin {
                horizontal: 2,
                vertical: 1,
            }),
        );
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

    fn help_options(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("s", "Start Background Worker"),
            ("t", "Stop Background Worker"),
            ("q", "Exit"),
        ]
    }
}
