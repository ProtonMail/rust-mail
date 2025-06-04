use crate::app::Command;
use crate::messages::Messages;
use flume::Sender;
use futures::FutureExt;
use futures::future::BoxFuture;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use tracing::error;

/// Handle which on drop terminates the observation of database changes.
pub struct WatchHandle {
    _sender: Sender<()>,
    _handle: DropRemoveTableObserverHandle,
}
impl WatchHandle {
    /// Create a new watcher which is not dampened.
    pub fn new<T: Send + 'static>(
        receiver: flume::Receiver<T>,
        handle: DropRemoveTableObserverHandle,
        converter: impl Fn(T) -> BoxFuture<'static, Option<Messages>> + Send + 'static,
    ) -> (Self, Command<Messages>) {
        let (control_sender, control_receiver) = flume::bounded(0);

        let command = Command::background_task(|background_sender| {
            async move {
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
            }.boxed()
        });
        (
            Self {
                _sender: control_sender,
                _handle: handle,
            },
            command,
        )
    }
}
