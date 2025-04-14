use crate::errors::unexpected::UnexpectedError;
use crate::errors::{EventError, EventErrorReason, ProtonError};
use crate::mail::MailUserSession;
use crate::spawn_async;
use proton_action_queue::observers::{ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::{ActionError, AsActionError};
use proton_event_loop::EventLoopError;
use proton_mail_common::actions::event_poll::{ActionEventLoopError, EventPoll};
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
        let handle = spawn_async(self.ctx()?, async move {
            while let Ok(v) = observer.next().await {
                if let ActionFailureReason::Error(err, _) = v {
                    let err = if let Some(details) = err.as_action_error::<EventPoll>() {
                        match details {
                            ActionError::Action(e) => match e {
                                ActionEventLoopError::EventLoop(EventLoopError::Refresh) => {
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
