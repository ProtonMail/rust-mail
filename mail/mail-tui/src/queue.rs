use proton_async::sync::mpsc::{unbounded, Receiver, Sender, TryRecvError};
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing;

pub struct DispatchQueue<T> {
    dispatcher: QueueDispatcher<T>,
    receiver: Receiver<DispatchObject<T>>,
}

impl<T> DispatchQueue<T> {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            receiver,
            dispatcher: QueueDispatcher(sender),
        }
    }

    pub fn dispatcher(&self) -> &QueueDispatcher<T> {
        &self.dispatcher
    }

    pub fn try_receive(&mut self) -> Option<DispatchObject<T>> {
        self.handle_try_receive(self.receiver.try_recv())
    }
    fn handle_try_receive(
        &mut self,
        r: Result<DispatchObject<T>, TryRecvError>,
    ) -> Option<DispatchObject<T>> {
        match r {
            Ok(o) => Some(o),
            Err(e) => {
                match e {
                    TryRecvError::Empty => {
                        // Nothing
                    }
                    TryRecvError::Disconnected => {
                        tracing::error!("Channel is closed")
                    }
                }
                None
            }
        }
    }
}

type DispatchObject<T> = Box<dyn FnOnce(&mut T) + Send>;
pub type LocalDispatchObject<T> = Box<dyn FnOnce(&mut T)>;
pub struct QueueDispatcher<T>(Sender<DispatchObject<T>>);

impl<T> Clone for QueueDispatcher<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> QueueDispatcher<T> {
    pub fn queue_sync<F: FnOnce(&mut T) + Send + 'static>(&self, t: F) {
        if let Err(e) = self.0.send(Box::new(t)) {
            tracing::error!("Failed to dispatch: {e}");
        }
    }
    pub async fn queue_async<F: FnOnce(&mut T) + Send + 'static>(&self, t: F) {
        if let Err(e) = self.0.send_async(Box::new(t)).await {
            tracing::error!("Failed to dispatch: {e}");
        }
    }
}
