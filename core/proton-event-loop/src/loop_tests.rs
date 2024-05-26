use crate::background_loop::MockEventLoopErrorHandler;
use crate::{
    BackgroundEventLoop, EventLoopError, EventLoopErrorHandlerReply, MockProvider, MockStore,
    MockSubscriber, Subscriber, SubscriberError,
};
use mockall::Sequence;
use proton_api_core::domain::{EventId, More};
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::spawn;

proton_api_core::declare_event!(LoopEvent,{f:bool});

#[tokio::test]
async fn test_loop_event_collection() {
    let first_event_id = EventId("0".into());
    let second_event_id = EventId("1".into());
    let third_event_id = EventId("2".into());

    let expected_events = [
        LoopEvent {
            event_id: second_event_id.clone(),
            more: More::Yes,
            f: false,
        },
        LoopEvent {
            event_id: third_event_id.clone(),
            more: More::No,
            f: false,
        },
    ];

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let error_handler = MockEventLoopErrorHandler::new();

    // Read store
    store
        .expect_load()
        .times(1)
        .in_sequence(&mut sequence)
        .return_once(|| Ok(None));

    // Collect events
    {
        let first_event_id = first_event_id.clone();
        provider
            .expect_get_latest_event_id()
            .times(1)
            .in_sequence(&mut sequence)
            .return_once(move || Ok(first_event_id.clone()));
    }

    {
        let first_event_id = first_event_id.clone();
        store
            .expect_store()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(|_| Ok(()));
    }

    {
        let first_event_id = first_event_id.clone();
        store
            .expect_load()
            .times(1)
            .in_sequence(&mut sequence)
            .return_once(move || Ok(Some(first_event_id)));
    }

    {
        let first_event_id = first_event_id.clone();
        let event = expected_events[0].clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| Ok(event));
    }

    {
        let second_event_id = second_event_id.clone();
        let event = expected_events[1].clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == second_event_id)
            .return_once(move |_| Ok(event));
    }

    // Publish events
    subscriber
        .expect_on_events()
        .times(1)
        .in_sequence(&mut sequence)
        .withf(move |events| events == expected_events.as_slice())
        .return_once(|_| Ok(()));

    subscriber.expect_name().return_const("foo".into());

    let eloop = BackgroundEventLoop::new();
    // store new event id
    {
        let loop_cloned = eloop.clone();
        let event_id = third_event_id.clone();
        store
            .expect_store()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == event_id)
            .return_once(move |_| {
                loop_cloned.cancel();
                Ok(())
            });
    }

    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber);
    let handle = eloop
        .start(
            Duration::from_secs(1),
            Box::new(store),
            Box::new(provider),
            Box::new(error_handler),
        )
        .await
        .expect("Failed to start event loop");

    eloop.resume();
    handle.await.expect("Expected no error on join");
}

#[tokio::test]
async fn test_error_handler_retry_retries_loop() {
    let first_event_id = EventId("0".into());
    let second_event_id = EventId("1".into());

    let expected_events = [LoopEvent {
        event_id: second_event_id.clone(),
        more: More::No,
        f: false,
    }];

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let mut error_handler = MockEventLoopErrorHandler::new();

    // Read store
    {
        let first_event_id = first_event_id.clone();
        store
            .expect_load()
            .times(2)
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(first_event_id.clone())));
    }

    {
        let first_event_id = first_event_id.clone();
        let event = expected_events[0].clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| Ok(event));
    }

    // Publish events
    {
        let expected_events = expected_events.clone();
        subscriber
            .expect_on_events()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |events| events == expected_events.as_slice())
            .return_once(|_| Err(SubscriberError::Other(anyhow!("Failed to apply event"))));
    }

    subscriber.expect_name().return_const("foo".into());

    let eloop = BackgroundEventLoop::new();

    error_handler
        .expect_on_error()
        .withf(|f| matches!(f, EventLoopError::Subscriber(_, _)))
        .times(1)
        .return_const(EventLoopErrorHandlerReply::Retry)
        .in_sequence(&mut sequence);

    // Re-fetch event.
    {
        let first_event_id = first_event_id.clone();
        store
            .expect_load()
            .times(1)
            .in_sequence(&mut sequence)
            .return_once(move || Ok(Some(first_event_id)));
    }
    {
        let first_event_id = first_event_id.clone();
        let event = expected_events[0].clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| Ok(event));
    }

    {
        let expected_events = expected_events.clone();
        subscriber
            .expect_on_events()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |events| events == expected_events.as_slice())
            .return_once(|_| Ok(()));
    }

    // store new event id
    {
        let loop_cloned = eloop.clone();
        let event_id = second_event_id.clone();
        store
            .expect_store()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == event_id)
            .return_once(move |_| {
                loop_cloned.cancel();
                Ok(())
            });
    }

    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber);
    let handle = eloop
        .start(
            Duration::from_secs(1),
            Box::new(store),
            Box::new(provider),
            Box::new(error_handler),
        )
        .await
        .expect("Failed to start event loop");

    eloop.resume();

    handle.await.expect("Expected no error on join");
}

#[tokio::test]
async fn test_error_handler_pause_pauses_loop() {
    let first_event_id = EventId("0".into());

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let mut error_handler = MockEventLoopErrorHandler::new();

    // Read store
    {
        let first_event_id = first_event_id.clone();
        store
            .expect_load()
            .times(2)
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(first_event_id.clone())));
    }

    {
        let first_event_id = first_event_id.clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| {
                Err(proton_api_core::http::RequestError::Other(anyhow!(
                    "Failure"
                )))
            });
    }

    subscriber.expect_name().return_const("foo".into());

    let eloop = BackgroundEventLoop::new();

    let loop_cloned = eloop.clone();
    error_handler
        .expect_on_error()
        .times(1)
        .return_once(|_| {
            let _ = spawn(async move {
                loop_cloned.cancel();
            });
            EventLoopErrorHandlerReply::Pause
        })
        .in_sequence(&mut sequence);

    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber);
    let handle = eloop
        .start(
            Duration::from_secs(1),
            Box::new(store),
            Box::new(provider),
            Box::new(error_handler),
        )
        .await
        .expect("Failed to start event loop");

    eloop.resume();

    handle.await.expect("Expected no error on join");
    assert!(eloop.is_paused());
}

#[tokio::test]
async fn test_error_handler_abort_causes_loop_exit() {
    let first_event_id = EventId("0".into());

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let mut error_handler = MockEventLoopErrorHandler::new();

    // Read store
    {
        let first_event_id = first_event_id.clone();
        store
            .expect_load()
            .times(2)
            .in_sequence(&mut sequence)
            .returning(move || Ok(Some(first_event_id.clone())));
    }

    {
        let first_event_id = first_event_id.clone();
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| {
                Err(proton_api_core::http::RequestError::Other(anyhow!(
                    "Failure"
                )))
            });
    }

    subscriber.expect_name().return_const("foo".into());

    error_handler
        .expect_on_error()
        .times(1)
        .return_const(EventLoopErrorHandlerReply::Abort)
        .in_sequence(&mut sequence);

    let eloop = BackgroundEventLoop::new();
    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber);
    let handle = eloop
        .start(
            Duration::from_secs(1),
            Box::new(store),
            Box::new(provider),
            Box::new(error_handler),
        )
        .await
        .expect("Failed to start event loop");

    eloop.resume();

    handle.await.expect("Expected no error on join");
}
