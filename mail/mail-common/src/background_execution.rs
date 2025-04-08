use crate::{MailContext, MailContextResult, MailUserContext};
use proton_task_service::TaskService;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

pub enum BackgroundExecutionStatus {
    /// Skipped due to the lack of logged in and initialized user contexts.
    SkippedNoActiveContexts,
    /// Actually executed something.
    Executed,
    /// Abort request triggered in background
    AbortedInBackground,
    /// Abort request triggered in foreground
    AbortedInForeground,
    /// We ran more than the allotted time.
    TimedOut,
}

/// Contains all relevant state to successfully execute actions in the background.
pub struct BackgroundExecutionContext {
    task_service: TaskService,
}

impl BackgroundExecutionContext {
    pub fn new() -> MailContextResult<Self> {
        Ok(Self {
            task_service: TaskService::new()?,
        })
    }

    /// Create new queue executors to run tasks separate from the main queue executors until
    /// `abort` returns.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn run(
        &self,
        ctx: &Arc<MailContext>,
        abort: impl Future<Output = bool>,
        max_duration: Duration,
    ) -> MailContextResult<BackgroundExecutionStatus> {
        tracing::debug!("Background execution is gathering user contexts");

        let all_user_ctxs = ctx
            .get_all_logged_in_and_initialized_user_contexts()
            .await
            .inspect_err(|e| {
                tracing::error!("Failed to get logged in users, details: `{e:?}`");
            })?;

        if all_user_ctxs.is_empty() {
            tracing::warn!("There are no logged in users, skipping background execution");
            return Ok(BackgroundExecutionStatus::SkippedNoActiveContexts);
        }

        tracing::debug!("Background execution is creating executors");

        // Create new executors for all the contexts.
        let queue_executors = all_user_ctxs
            .iter()
            .map(|ctx| {
                let online = ctx.session().status_watcher().subscribe_to_online();

                let send_executor = MailUserContext::new_background_send_queue_executor(
                    ctx.action_queue(),
                    online.clone(),
                    NonZeroUsize::new(2).unwrap(),
                    &self.task_service,
                );

                let default_executor = MailUserContext::new_background_default_queue_executor(
                    ctx.action_queue(),
                    online,
                    &self.task_service,
                );

                (send_executor, default_executor)
            })
            .collect::<Vec<_>>();

        tracing::debug!("Background execution is in progress... awaiting for abort");
        let status = {
            // scoped here to force drop of the executors.
            let await_queue_executors = async {
                for (send_executor, default_executor) in queue_executors {
                    send_executor.await_finished().await;
                    default_executor.await_finished().await;
                }
            };
            let timeout = tokio::time::sleep(max_duration);
            tokio::select! {
                _ = await_queue_executors=> {
                    BackgroundExecutionStatus::Executed
                },
                _ = timeout => {
                    tracing::debug!("Background execution timed out");
                    BackgroundExecutionStatus::TimedOut
                },
                in_foreground = abort => {
                    if in_foreground {
                        BackgroundExecutionStatus::AbortedInForeground
                    } else {
                        BackgroundExecutionStatus::AbortedInBackground
                    }
                }
            }
        };
        // Pause all executors and make sure all non-pausable futures finish on time.
        tracing::info!("Pausing Background queue executors...");
        if self.task_service.pause_and_wait().await.is_err() {
            tracing::error!("Pausing Background queue executors... Failed");
        } else {
            tracing::info!("Pausing executors... Done");
        }
        tracing::info!("Background execution finished");
        Ok(status)
    }
}
