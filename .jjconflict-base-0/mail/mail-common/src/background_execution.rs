use crate::user_context::DefaultQueueExecutor;
use crate::{MailContext, MailContextResult, MailUserContext};
use std::sync::Arc;
use std::time::Duration;
use tracing::error;

#[derive(Debug, Copy, Clone)]
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

pub struct BackgroundExecutionResult {
    pub status: BackgroundExecutionStatus,
    pub has_unsent_messages: bool,
    pub has_pending_actions: bool,
}

impl BackgroundExecutionResult {
    fn no_active_contexts() -> Self {
        Self {
            status: BackgroundExecutionStatus::SkippedNoActiveContexts,
            has_unsent_messages: false,
            has_pending_actions: false,
        }
    }
}

/// Contains all relevant state to successfully execute actions in the background.
pub struct BackgroundExecutionContext {}

impl BackgroundExecutionContext {
    #[allow(clippy::result_large_err)]
    pub fn new() -> MailContextResult<Self> {
        Ok(Self {})
    }

    /// Create new queue executors to run tasks separate from the main queue executors until
    /// `abort` returns.
    #[tracing::instrument(skip_all)]
    pub async fn run(
        &self,
        ctx: &Arc<MailContext>,
        abort: impl Future<Output = bool>,
        max_duration: Duration,
    ) -> MailContextResult<BackgroundExecutionResult> {
        tracing::debug!("Background execution is gathering user contexts");

        let all_user_ctxs = ctx
            .get_all_logged_in_and_initialized_user_contexts()
            .await
            .inspect_err(|e| {
                tracing::error!("Failed to get logged in users, details: `{e:?}`");
            })?;

        if all_user_ctxs.is_empty() {
            tracing::warn!("There are no logged in users, skipping background execution");
            return Ok(BackgroundExecutionResult::no_active_contexts());
        }

        let _pause_prefetch_rollback = PausePrefetchRollbackScope::new(&all_user_ctxs);

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
        if let Err(e) = ctx
            .core_context()
            .task_service()
            .pause_background_and_wait(Duration::from_millis(100))
            .await
        {
            tracing::warn!("Pausing Background queue executors... Failed: {e:?}");
        } else {
            tracing::info!("Pausing executors... Done");
        }
        tracing::info!("Background execution finished");

        let mut has_unsent_messages = false;
        let mut has_pending_actions = false;
        for ctx in &all_user_ctxs {
            has_unsent_messages = has_unsent_messages
                || ctx
                    .has_unsent_messages()
                    .await
                    .inspect_err(|e| {
                        error!(
                            "Failed to check {} for unsent messages: {e:?}",
                            ctx.user_id()
                        )
                    })
                    .unwrap_or(false);
            has_pending_actions = has_pending_actions
                || ctx
                    .has_actions_in_queue()
                    .await
                    .inspect_err(|e| {
                        error!(
                            "Failed to check {} for unprocessed actions: {e:?}",
                            ctx.user_id()
                        )
                    })
                    .unwrap_or(false);
        }

        Ok(BackgroundExecutionResult {
            status,
            has_unsent_messages,
            has_pending_actions,
        })
    }
}

/// Interface for avoiding running prefetch and rollback while background execution is running.
pub struct PausePrefetchRollbackScope<'a> {
    ctxs: &'a [Arc<MailUserContext>],
}

impl<'a> PausePrefetchRollbackScope<'a> {
    pub fn new(ctxs: &'a [Arc<MailUserContext>]) -> Self {
        for ctx in ctxs {
            ctx.get_service::<DefaultQueueExecutor>()
                .pause_prefetch_rollback();
        }
        Self { ctxs }
    }

    pub fn resume(&self) {
        for ctx in self.ctxs {
            ctx.get_service::<DefaultQueueExecutor>()
                .resume_prefetch_rollback();
        }
    }
}

impl<'a> Drop for PausePrefetchRollbackScope<'a> {
    fn drop(&mut self) {
        self.resume();
    }
}
