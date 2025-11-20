use crate::async_runtime;
use crate::errors::UserSessionError;
use crate::mail::MailSession;
use proton_mail_common::MailContext;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::background_execution::{
    BackgroundExecutionContext, BackgroundExecutionResult as RealBackgroundExecutionResult,
    BackgroundExecutionStatus as RealBackgroundExecutionStatus,
};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::mpsc;

#[uniffi_export]
impl MailSession {
    /// Functionality to execute pending actions for all logged in accounts in controlled manner.
    ///
    /// This method is meant to be executed when putting application to sleep or running it in the background.
    ///
    /// It will stop when aborted or when finished whatever comes first.
    /// On exit the callback will be triggered to notify caller that it finished.
    ///
    /// A default time out of 30 seconds is assigned to this method, for more control use
    /// [`start_background_execution_with_duration`].
    #[tracing::instrument(skip_all)]
    pub fn start_background_execution(
        &self,
        callback: Arc<dyn BackgroundExecutionCallback>,
    ) -> Result<Arc<BackgroundExecutionHandle>, UserSessionError> {
        self.start_background_execution_with_duration_impl(Duration::from_secs(30), callback)
    }

    /// Same as [`start_background_execution`] but an optional `duration_seconds` can be specified.
    ///
    /// Note that the duration is the maximum time we will wait for either the background work
    /// to finish or the abort handle to be called. We can still  spend some time after that
    /// waiting for task completion.
    #[tracing::instrument(skip_all)]
    pub fn start_background_execution_with_duration(
        &self,
        duration_seconds: u64,
        callback: Arc<dyn BackgroundExecutionCallback>,
    ) -> Result<Arc<BackgroundExecutionHandle>, UserSessionError> {
        self.start_background_execution_with_duration_impl(
            Duration::from_secs(duration_seconds),
            callback,
        )
    }
}
impl MailSession {
    /// See [`start_background_execution_with_duration`] for details.
    fn start_background_execution_with_duration_impl(
        &self,
        duration: Duration,
        callback: Arc<dyn BackgroundExecutionCallback>,
    ) -> Result<Arc<BackgroundExecutionHandle>, UserSessionError> {
        let ctx = self.ctx_arc();
        let (sender, mut abort) = mpsc::channel(1);
        let background_context =
            BackgroundExecutionContext::new().map_err(RealProtonMailError::from)?;
        // This task needs to run a free task that won't get paused or it may get stuck.
        async_runtime().spawn(async move {
            let result = match background_context
                .run(
                    &ctx,
                    async { abort.recv().await.unwrap_or(false) },
                    duration,
                )
                .await
            {
                Ok(s) => s.into(),
                Err(e) => BackgroundExecutionResult {
                    has_unsent_messages: false,
                    has_pending_actions: false,
                    status: BackgroundExecutionStatus::Failed(e.to_string()),
                },
            };
            callback.on_execution_completed(result).await;
        });

        Ok(Arc::new(BackgroundExecutionHandle {
            sender,
            ctx: Arc::downgrade(&self.ctx_arc()),
        }))
    }
}

#[derive(uniffi::Enum)]
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
    /// Failed to execute
    Failed(String),
}

impl From<RealBackgroundExecutionStatus> for BackgroundExecutionStatus {
    fn from(value: RealBackgroundExecutionStatus) -> Self {
        match value {
            RealBackgroundExecutionStatus::SkippedNoActiveContexts => Self::SkippedNoActiveContexts,
            RealBackgroundExecutionStatus::Executed => Self::Executed,
            RealBackgroundExecutionStatus::AbortedInBackground => Self::AbortedInBackground,
            RealBackgroundExecutionStatus::AbortedInForeground => Self::AbortedInForeground,
            RealBackgroundExecutionStatus::TimedOut => Self::TimedOut,
        }
    }
}

#[derive(uniffi::Record)]
pub struct BackgroundExecutionResult {
    pub status: BackgroundExecutionStatus,
    pub has_unsent_messages: bool,
    pub has_pending_actions: bool,
}

impl From<RealBackgroundExecutionResult> for BackgroundExecutionResult {
    fn from(value: RealBackgroundExecutionResult) -> Self {
        Self {
            status: value.status.into(),
            has_unsent_messages: value.has_unsent_messages,
            has_pending_actions: value.has_pending_actions,
        }
    }
}
/// Callback to be notified when background execution completes.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait BackgroundExecutionCallback: Send + Sync {
    /// Called when the background execution has terminated.
    ///
    /// Check the returned `result` for more details.
    async fn on_execution_completed(&self, result: BackgroundExecutionResult);
}

/// Handle for background activites execution.
///
/// It is meant to be hold by a caller of `start_background_execution` method.
/// When dropped it will cease the execution.
///
#[derive(uniffi::Object)]
pub struct BackgroundExecutionHandle {
    sender: mpsc::Sender<bool>,
    ctx: Weak<MailContext>,
}

#[uniffi_export]
impl BackgroundExecutionHandle {
    /// Abort background execution.
    ///
    /// Allows holder of the `BackgroundExecutionHandle` to finish execution prematurely.
    ///
    pub async fn abort(&self, in_foreground: bool) {
        let _ = self.sender.send(in_foreground).await;
    }
}

impl Drop for BackgroundExecutionHandle {
    fn drop(&mut self) {
        let sender = self.sender.clone();
        if let Some(ctx) = self.ctx.upgrade() {
            ctx.spawn(async move {
                let _ = sender.send(false).await;
            });
        } else {
            tracing::warn!(
                "MailContext already dropped, background execution handle should not live that long"
            );
        }
    }
}
