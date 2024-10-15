use crate::app::Command;
use crate::messages::Messages;
use flume::Sender;
use futures::future::BoxFuture;
use tracing::error;

/// Handle which on drop terminates the observation of database changes.
pub struct WatchHandle {
    _sender: Sender<()>,
}
impl WatchHandle {
    /// Create a new watcher which is not dampened.
    pub fn new<T: Send + 'static>(
        receiver: flume::Receiver<T>,
        converter: impl Fn(T) -> BoxFuture<'static, Option<Messages>> + Send + 'static,
        background_sender: Sender<Command<Messages>>,
    ) -> Self {
        let (control_sender, control_receiver) = flume::bounded(0);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = control_receiver.recv_async() => {
                        // Terminate watcher
                        return;
                    }

                    result = receiver.recv_async() => {
                        let Ok(value) = result else {
                            return;
                        };

                        if let Some(message) = converter(value).await {
                            if background_sender.send_async(Command::message(message)).await.is_err() {
                                error!("Failed to send message from watcher");
                                return;
                            }
                        }
                    }
                }
            }
        });
        Self {
            _sender: control_sender,
        }
    }

    /// Create a new watcher that is dampened.
    pub fn new_dampened<T: Send + 'static>(
        receiver: flume::Receiver<T>,
        converter: impl Fn() -> BoxFuture<'static, Option<Messages>> + Send + 'static,
        background_sender: Sender<Command<Messages>>,
    ) -> Self {
        let (control_sender, control_receiver) = flume::bounded(0);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            let mut received_update = false;
            loop {
                tokio::select! {
                    _ = control_receiver.recv_async() => {
                        // Terminate watcher
                        return;
                    }
                    _ = interval.tick() => {
                        if received_update {
                            received_update = false;

                            if let Some(message) = converter().await {
                                if background_sender.send_async(Command::message(message)).await.is_err() {
                                    error!("Failed to send message from watcher");
                                    return;
                                }
                            }
                        }
                    }

                    result = receiver.recv_async() => {
                        let Ok(_) = result else {
                            return;
                        };

                        received_update = true;
                    }
                }
            }
        });
        Self {
            _sender: control_sender,
        }
    }
}
