use crate::errors::unexpected::UnexpectedError;
use crate::errors::{EventError, EventErrorReason, ProtonError, VoidEventResult};
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_action_queue::observers::{ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::{ActionError, AsActionError};
use proton_core_common::actions::event_poll::{ActionEventLoopError, EventPoll};
use proton_event_loop::EventLoopError;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::Unexpected;
use std::sync::Arc;
use tokio::task::AbortHandle;

/// Event loop error observer callback.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait EventLoopErrorObserver: Send + Sync {
    /// Invoked when the event loop runs into an error that prevents it from progressing.
    async fn on_event_loop_error(&self, error: EventError);
}

/// Handle returned when observing event loop errors.
///
/// Keep this handle alive to maintain the callback alive.
#[derive(uniffi::Object)]
pub struct EventLoopErrorObserverHandle(AbortHandle);

impl Drop for EventLoopErrorObserverHandle {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[uniffi_export]
impl EventLoopErrorObserverHandle {
    /// Disconnect this observer and release all associated resources.
    pub fn disconnect(&self) {
        self.0.abort();
    }
}

#[uniffi_export]
impl MailUserSession {
    /// Queue an event loop poll action regardless of polling mode.
    ///
    /// # Errors
    ///
    /// Errors returned here are only related to queuing of the action. To get information
    /// about the event loop execution, please use [`MailUserSession::observe_event_loop_errors`].
    #[returns(VoidEventResult)]
    pub async fn force_event_loop_poll(&self) -> Result<(), EventError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            ctx.force_event_loop_poll()
                .await
                .map_err(|_| RealProtonMailError::Unexpected(Unexpected::Internal))?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(EventError::from)
        .into()
    }

    #[returns(VoidEventResult)]
    pub async fn force_event_loop_poll_and_wait(&self) -> Result<(), EventError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            ctx.force_event_loop_poll_and_wait()
                .await
                .map_err(|_| RealProtonMailError::Unexpected(Unexpected::Internal))?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(EventError::from)
        .into()
    }

    /// Observe event loop errors.
    ///
    /// When an error occurs the `callback` is invoked.
    ///
    /// This method returns an [`EventLoopErrorObserverHandle`] which needs to be kept
    /// alive. Once the handle is disconnected or dropped, the `callback` is removed.
    pub fn observe_event_loop_errors(
        &self,
        callback: Arc<dyn EventLoopErrorObserver>,
    ) -> Result<Arc<EventLoopErrorObserverHandle>, EventError> {
        let mut observer = ActionFailureObserver::<EventPoll>::new(self.ctx()?.action_queue());
        let handle = self.ctx()?.spawn(async move {
            while let Ok(v) = observer.next().await {
                if let ActionFailureReason::Error(err, _) = v {
                    let err = if let Some(details) = err.as_action_error::<EventPoll>() {
                        tracing::error!(?details, "Reporting event loop error");
                        match details {
                            ActionError::Action(e) => match e {
                                ActionEventLoopError::EventLoop(
                                    EventLoopError::Refresh(_, e)
                                    | EventLoopError::Subscriber(_, e),
                                ) if e.is_retryable() => {
                                    // if the error is retryable do not communicate this to the
                                    // user.
                                    continue;
                                }
                                ActionEventLoopError::EventLoop(EventLoopError::Refresh(_, _)) => {
                                    EventError::Reason(EventErrorReason::Refresh)
                                }
                                ActionEventLoopError::EventLoop(EventLoopError::Subscriber(
                                    _,
                                    _,
                                )) => EventError::Reason(EventErrorReason::Subscriber),
                                _ => EventError::Other(ProtonError::Unexpected(
                                    UnexpectedError::Internal,
                                )),
                            },
                            ActionError::Queue(_) => {
                                EventError::Other(ProtonError::Unexpected(UnexpectedError::Queue))
                            }
                        }
                    } else {
                        tracing::error!(?err, "Reporting event loop error");
                        EventError::Other(ProtonError::Unexpected(UnexpectedError::Unknown))
                    };
                    callback.on_event_loop_error(err).await;
                }
            }
        });

        Ok(Arc::new(EventLoopErrorObserverHandle(
            handle.abort_handle(),
        )))
    }
}
