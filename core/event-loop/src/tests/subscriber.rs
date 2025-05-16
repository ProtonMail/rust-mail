// #![allow(non_snake_case)]

// use super::*;
// use proton_core_api::services::proton::EventId;
// use serde::Deserialize;

// const DUMMY_EVENT_ID: &str = "EVT_FOO";

// fn new_dummy_events() -> Vec<TestEvent> {
//     vec![TestEvent {
//         event_id: EventId::from(DUMMY_EVENT_ID),
//         has_more: false,
//         foo: 0,
//     }]
// }

// #[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
// pub struct TestEvent {
//     pub event_id: EventId,
//     pub foo: u32,
//     pub has_more: bool,
// }

// impl Event for TestEvent {
//     type Response = TestEvent;

//     fn event_id(&self) -> &EventId {
//         &self.event_id
//     }

//     fn has_more(&self) -> bool {
//         self.has_more
//     }

//     fn is_refresh(&self) -> bool {
//         false
//     }
// }

// #[tokio::test]
// async fn test_channeled_subscriber_handle_and_reply() {
//     let (s, mut h) = ChannelledSubscriber::new("test".into());

//     let task = tokio::spawn(async move {
//         h.handle_events_async(|events: &[TestEvent]| -> Result<(), SubscriberError> {
//             assert_eq!(events[0].event_id, EventId::from(DUMMY_EVENT_ID));
//             Ok(())
//         })
//         .await
//         .expect("failed to handle event");
//     });
//     let mut events = new_dummy_events();
//     s.on_events(&mut events)
//         .await
//         .expect("failed handle events");

//     task.await.expect("expected no error on join");
// }

// #[tokio::test]
// async fn test_channeled_subscriber_failed_to_send() {
//     let s = {
//         let (s, _) = ChannelledSubscriber::new("test".into());
//         s
//     };

//     let mut events = new_dummy_events();
//     assert!(matches!(
//         s.on_events(&mut events).await.expect_err("expected error"),
//         SubscriberError::Send
//     ));
// }

// #[tokio::test]
// async fn test_channeled_subscriber_failed_to_receive() {
//     let (s, h) = ChannelledSubscriber::new("test".into());

//     let task = tokio::spawn(async move {
//         h.receiver
//             .recv_async()
//             .await
//             .expect("expected to receive data");
//         drop(h);
//     });
//     let mut events = new_dummy_events();
//     assert!(matches!(
//         s.on_events(&mut events).await.expect_err("expected error"),
//         SubscriberError::Receive
//     ));

//     task.await.expect("expected no error on join");
// }

// #[tokio::test]
// async fn test_channeled_subscriber_handler_failed_to_receive() {
//     let mut h = {
//         let (_, h) = ChannelledSubscriber::new("test".into());
//         h
//     };

//     assert!(matches!(
//         h.handle_events_async(|_: &[TestEvent]| -> Result<(), SubscriberError> { Ok(()) })
//             .await
//             .expect_err("expected error"),
//         ChanneledSubscriberError::Receive
//     ));
// }

// #[tokio::test]
// async fn test_channeled_subscriber_handler_failed_to_send() {
//     let (s, mut h) = ChannelledSubscriber::new("test".into());

//     let task = tokio::spawn(async move {
//         let events = new_dummy_events();
//         s.sender.send_async(events).await.expect("failed to send");
//         drop(s);
//     });

//     task.await.expect("expected no error on join");
//     assert!(matches!(
//         h.handle_events_async(|_| -> Result<(), SubscriberError> { Ok(()) })
//             .await
//             .expect_err("expected error"),
//         ChanneledSubscriberError::Send(_)
//     ));
// }
