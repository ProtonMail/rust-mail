#![allow(clippy::module_name_repetitions)] // avoid namespace conflicts
use proton_api_core::domain::Event;
use proton_api_core::exports::anyhow;
#[cfg(test)]
use proton_api_core::exports::serde;
#[cfg(test)]
use proton_api_core::exports::serde::{Deserialize, Serialize};
use proton_api_core::exports::thiserror;
use proton_async::async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum SubscriberError {
    /// Http error should be returned when the error resulted due to an API or Network error.
    #[error("{0}")]
    Http(proton_api_core::http::RequestError),
    /// Subscriber specific errors should be returned here.
    #[error("{0}")]
    Other(anyhow::Error),
    /// Failed to send to the subscriber.
    #[error("Failed to send data to subscriber")]
    Send,
    /// Failed to receive data from subscriber.
    #[error("Failed to receive data from subscriber")]
    Receive,
}

/// Subscriber traits allow anyone to access the events from the event loop.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Subscriber<T: Event + Send + Sync>: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &str;

    /// Handle incoming events.
    async fn on_events(&self, event: &[T]) -> Result<(), SubscriberError>;
}

/// A Subscriber in which all event communication is performed via channels. This may be useful if your subscribe is
/// running on another task and do not wish to make the state sharable.
pub struct ChannelledSubscriber<T: Event + Send + Sync> {
    name: String,
    sender: proton_async::sync::mpsc::Sender<Vec<T>>,
    receiver: proton_async::sync::mpsc::Receiver<Result<(), SubscriberError>>,
}

#[async_trait]
impl<T: Event + Send + Sync> Subscriber<T> for ChannelledSubscriber<T> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_events(&self, event: &[T]) -> Result<(), SubscriberError> {
        if self.sender.send_async(Vec::from(event)).await.is_err() {
            return Err(SubscriberError::Send);
        }

        let Ok(reply) = self.receiver.recv_async().await else {
            return Err(SubscriberError::Receive);
        };

        reply
    }
}

impl<T: Event> ChannelledSubscriber<T> {
    #[must_use]
    pub fn new(name: String) -> (ChannelledSubscriber<T>, ChanneledSubscriberHandler<T>) {
        let (subscriber_sender, subscriber_receiver) = proton_async::sync::mpsc::bounded(1);
        let (handler_sender, handler_receiver) = proton_async::sync::mpsc::bounded(1);

        (
            ChannelledSubscriber {
                name,
                sender: handler_sender,
                receiver: subscriber_receiver,
            },
            ChanneledSubscriberHandler {
                receiver: handler_receiver,
                sender: subscriber_sender,
            },
        )
    }
}

/// `ChanneledSubscriberHandler` waits on events to be send over a channel. These can then be consumed by the
/// `handle_events` function.
pub struct ChanneledSubscriberHandler<T: Event> {
    receiver: proton_async::sync::mpsc::Receiver<Vec<T>>,
    sender: proton_async::sync::mpsc::Sender<Result<(), SubscriberError>>,
}

/// Error returned by `ChanneledSubscriberHandler` which includes errors when receiving events or transmitting
/// replies.
#[derive(Debug, thiserror::Error)]
pub enum ChanneledSubscriberError {
    /// Failed to receive events from the channel.
    #[error("Failed to receive events from channel")]
    Receive,
    /// Failed to send the reply back to the event loop
    #[error("Failed to send result on channel")]
    Send(Result<(), SubscriberError>),
}
impl<T: Event> ChanneledSubscriberHandler<T> {
    /// Handle the events from the event loop.
    ///
    /// # Errors
    /// Returns error if the subscriber failed to handle the events or the communication over
    /// the channel failed.
    pub async fn handle_events_async<Error: Into<SubscriberError>>(
        &mut self,
        mut f: impl FnMut(&[T]) -> Result<(), Error>,
    ) -> Result<(), ChanneledSubscriberError> {
        let Ok(events) = self.receiver.recv_async().await else {
            return Err(ChanneledSubscriberError::Receive);
        };

        let r = (f)(&events);

        self.reply_async(r.map_err(Into::into)).await
    }

    /// Handle the events from the event loop.
    ///
    /// # Errors
    /// Returns error if the subscriber failed to handle the events or the communication over
    /// the channel failed.
    pub fn handle_events<Error: Into<SubscriberError>>(
        &mut self,
        mut f: impl FnMut(&[T]) -> Result<(), Error>,
    ) -> Result<(), ChanneledSubscriberError> {
        let Ok(events) = self.receiver.recv() else {
            return Err(ChanneledSubscriberError::Receive);
        };

        let r = (f)(&events);

        self.reply(r.map_err(Into::into))
    }

    /// Receive events from event loop.
    /// Note: Each call to `receive` must have an `reply` counter part.
    pub async fn receive_async(&mut self) -> Option<Vec<T>> {
        if let Ok(v) = self.receiver.recv_async().await {
            return Some(v);
        }

        None
    }

    /// Receive events from event loop.
    /// Note: Each call to `receive` must have an `reply` counter part.
    pub fn receive(&mut self) -> Option<Vec<T>> {
        if let Ok(v) = self.receiver.recv() {
            return Some(v);
        }

        None
    }

    /// Report the result of handling `receive` to the event loop.
    ///
    /// # Errors
    /// Returns error if the reply could not be sent over the channel.
    pub async fn reply_async(
        &self,
        reply: Result<(), SubscriberError>,
    ) -> Result<(), ChanneledSubscriberError> {
        if let Err(e) = self.sender.send_async(reply).await {
            return Err(ChanneledSubscriberError::Send(e.0));
        }

        Ok(())
    }

    /// Report the result of handling `receive` to the event loop.
    ///
    /// # Errors
    /// Returns error if the reply could not be sent over the channel.
    pub fn reply(
        &self,
        reply: Result<(), SubscriberError>,
    ) -> Result<(), ChanneledSubscriberError> {
        if let Err(e) = self.sender.send(reply) {
            return Err(ChanneledSubscriberError::Send(e.0));
        }

        Ok(())
    }
}

#[cfg(test)]
proton_api_core::declare_event!(TestEvent,{foo:u32});

#[test]
fn test_channeled_subscriber_handle_and_reply() {
    use proton_api_core::domain::EventId;
    let rt = proton_async::runtime::MultiThreaded::new(2).expect("failed to create runtime");
    rt.block_on(async {
        let (s, mut h) = ChannelledSubscriber::new("test".into());

        let task = rt.spawn(async move {
            h.handle_events_async(|events: &[TestEvent]| -> Result<(), SubscriberError> {
                assert_eq!(events[0].event_id, EventId::from(DUMMY_EVENT_ID));
                Ok(())
            })
            .await
            .expect("failed to handle event");
        });
        let events = new_dummy_events();
        s.on_events(&events).await.expect("failed handle events");

        task.await.expect("expected no error on join");
    })
}

#[test]
fn test_channeled_subscriber_failed_to_send() {
    let rt = proton_async::runtime::InPlace::new().expect("failed to init runtime");
    rt.block_on(async {
        let s = {
            let (s, _) = ChannelledSubscriber::new("test".into());
            s
        };

        let events = new_dummy_events();
        assert!(matches!(
            s.on_events(&events).await.expect_err("expected error"),
            SubscriberError::Send
        ));
    });
}

#[test]
fn test_channeled_subscriber_failed_to_receive() {
    let rt = proton_async::runtime::MultiThreaded::new(2).expect("failed to create runtime");
    rt.block_on(async {
        let (s, h) = ChannelledSubscriber::new("test".into());

        let task = rt.spawn(async move {
            h.receiver
                .recv_async()
                .await
                .expect("expected to receive data");
            drop(h);
        });
        let events = new_dummy_events();
        assert!(matches!(
            s.on_events(&events).await.expect_err("expected error"),
            SubscriberError::Receive
        ));

        task.await.expect("expected no error on join");
    });
}

#[test]
fn test_channeled_subscriber_handler_failed_to_receive() {
    let rt = proton_async::runtime::InPlace::new().expect("failed to init runtime");
    rt.block_on(async {
        let mut h = {
            let (_, h) = ChannelledSubscriber::new("test".into());
            h
        };

        assert!(matches!(
            h.handle_events_async(|_: &[TestEvent]| -> Result<(), SubscriberError> { Ok(()) })
                .await
                .expect_err("expected error"),
            ChanneledSubscriberError::Receive
        ));
    });
}

#[test]
fn test_channeled_subscriber_handler_failed_to_send() {
    let rt = proton_async::runtime::MultiThreaded::new(2).expect("failed to create runtime");

    rt.block_on(async {
        let (s, mut h) = ChannelledSubscriber::new("test".into());

        let task = rt.spawn(async move {
            let events = new_dummy_events();
            s.sender.send_async(events).await.expect("failed to send");
            drop(s);
        });

        task.await.expect("expected no error on join");
        assert!(matches!(
            h.handle_events_async(|_| -> Result<(), SubscriberError> { Ok(()) })
                .await
                .expect_err("expected error"),
            ChanneledSubscriberError::Send(_)
        ));
    })
}

#[cfg(test)]
const DUMMY_EVENT_ID: &str = "EVT_FOO";

#[cfg(test)]
fn new_dummy_events() -> Vec<TestEvent> {
    use proton_api_core::domain::{EventId, More};
    vec![TestEvent {
        event_id: EventId::from(DUMMY_EVENT_ID),
        more: More::No,
        foo: 0,
    }]
}
