use crate::r#loop::MockLoopErrorHandler;
use crate::{
    Loop, LoopError, LoopErrorHandlerReply, MockProvider, MockStore, MockSubscriber, Subscriber,
    SubscriberError,
};
use mockall::Sequence;
use proton_api_core::domain::{EventId, MoreEvents};
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde::{Deserialize, Serialize};
use proton_async::tokio;
use std::time::Duration;

proton_api_core::declare_event!(LoopEvent,{f:bool});

#[tokio::test]
async fn test_loop_event_collection() {
    let first_event_id = EventId("0".into());
    let second_event_id = EventId("1".into());
    let third_event_id = EventId("2".into());

    let expected_events = [
        LoopEvent {
            event_id: second_event_id.clone(),
            more: MoreEvents::Yes,
            f: false,
        },
        LoopEvent {
            event_id: third_event_id.clone(),
            more: MoreEvents::No,
            f: false,
        },
    ];

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let error_handler = MockLoopErrorHandler::new();

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

    let eloop = Loop::new();
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
    eloop.subscribe(subscriber).await;
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
        more: MoreEvents::No,
        f: false,
    }];

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let mut error_handler = MockLoopErrorHandler::new();

    // Read store
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

    let eloop = Loop::new();

    error_handler
        .expect_on_error()
        .withf(|f| matches!(f, LoopError::Subscriber(_, _)))
        .times(1)
        .return_const(LoopErrorHandlerReply::Retry)
        .in_sequence(&mut sequence);

    // Re-fetch event.
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
    eloop.subscribe(subscriber).await;
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
    let mut error_handler = MockLoopErrorHandler::new();

    // Read store
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
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| {
                Err(proton_api_core::http::HttpRequestError::Other(anyhow!(
                    "Failure"
                )))
            });
    }

    subscriber.expect_name().return_const("foo".into());

    let eloop = Loop::new();

    let loop_cloned = eloop.clone();
    error_handler
        .expect_on_error()
        .times(1)
        .return_once(|_| {
            tokio::spawn(async move {
                loop_cloned.cancel();
            });
            LoopErrorHandlerReply::Pause
        })
        .in_sequence(&mut sequence);

    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber).await;
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
    let mut error_handler = MockLoopErrorHandler::new();

    // Read store
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
        provider
            .expect_get_event()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == first_event_id)
            .return_once(move |_| {
                Err(proton_api_core::http::HttpRequestError::Other(anyhow!(
                    "Failure"
                )))
            });
    }

    subscriber.expect_name().return_const("foo".into());

    error_handler
        .expect_on_error()
        .times(1)
        .return_const(LoopErrorHandlerReply::Abort)
        .in_sequence(&mut sequence);

    let eloop = Loop::new();
    let subscriber: Box<dyn Subscriber<LoopEvent>> = Box::new(subscriber);
    eloop.subscribe(subscriber).await;
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
