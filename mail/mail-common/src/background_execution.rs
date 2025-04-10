use crate::{MailContext, MailContextResult};
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
pub struct BackgroundExecutionContext {}

impl BackgroundExecutionContext {
    pub fn new() -> MailContextResult<Self> {
        Ok(Self {})
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

        ctx.core_context().task_service().resume_background();

        tracing::debug!("Background execution is in progress... awaiting for abort");
        let status = match tokio::time::timeout(max_duration, abort).await {
            Ok(true) => BackgroundExecutionStatus::AbortedInForeground,
            Ok(false) => BackgroundExecutionStatus::AbortedInBackground,
            Err(_) => {
                tracing::debug!("Background execution timed out");
                BackgroundExecutionStatus::TimedOut
            }
        };
        // Pause all executors and make sure all non-pausable futures finish on time.
        tracing::info!("Pausing Background queue executors...");
        if ctx
            .core_context()
            .task_service()
            .pause_background_and_wait()
            .await
            .is_err()
        {
            tracing::error!("Pausing Background queue executors... Failed");
        } else {
            tracing::info!("Pausing executors... Done");
        }
        tracing::info!("Background execution finished");
        Ok(status)
    }
}
