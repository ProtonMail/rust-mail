use crate::{Loop, MockProvider, MockStore, MockSubscriber, Subscriber};
use mockall::Sequence;
use proton_api_rs::domain::{Event, EventId, MoreEvents};
use proton_async::tokio;
use proton_async::tokio_util::sync::CancellationToken;
use std::time::Duration;

#[tokio::test]
async fn test_loop_event_collection() {
    let first_event_id = EventId("0".into());
    let second_event_id = EventId("1".into());
    let third_event_id = EventId("2".into());

    let expected_events = [
        Event {
            event_id: second_event_id.clone(),
            more: MoreEvents::Yes,
            messages: None,
            labels: None,
        },
        Event {
            event_id: third_event_id.clone(),
            more: MoreEvents::No,
            messages: None,
            labels: None,
        },
    ];

    let mut sequence = Sequence::new();
    let mut store = MockStore::new();
    let mut subscriber = MockSubscriber::new();
    let mut provider = MockProvider::new();
    let token = CancellationToken::new();

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

    // store new event id
    {
        let token = token.clone();
        let event_id = third_event_id.clone();
        store
            .expect_store()
            .times(1)
            .in_sequence(&mut sequence)
            .withf(move |id| *id == event_id)
            .return_once(move |_| {
                token.cancel();
                Ok(())
            });
    }

    let mut eloop = Loop::new(Box::new(store), Box::new(provider));

    let subscriber: Box<dyn Subscriber> = Box::new(subscriber);
    eloop
        .run(token, Duration::from_secs(1), [subscriber])
        .await
        .expect("Failed to run event loop");
}
