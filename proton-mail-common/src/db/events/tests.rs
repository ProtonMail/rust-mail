use crate::db::new_test_connection;
use proton_api_mail::proton_api_core::domain::EventId;

#[tokio::test]
async fn test_event_id_store_get_set() {
    let (_, mut conn, _) = new_test_connection();
    let tx = conn.transaction().await.expect("Failed to start transaction");
        let event_id1 = EventId::from("EVENT1");
        let event_id2 = EventId::from("EVENT2");
        let event_id3 = EventId::from("EVENT3");
        const EVENT_TYPE_ID_2: &str = "EVENT_TYPE";
        const EVENT_TYPE_ID_1: &str = "EVENT_TYPE_2";

        assert!(tx
            .get_last_event_id(EVENT_TYPE_ID_1)
            .expect("failed to get event id")
            .is_none());
        tx.set_last_event_id(EVENT_TYPE_ID_1, &event_id1)
            .expect("failed to set event id");
        assert_eq!(
            tx.get_last_event_id(EVENT_TYPE_ID_1)
                .expect("failed to get event id"),
            Some(event_id1)
        );
        tx.set_last_event_id(EVENT_TYPE_ID_1, &event_id2)
            .expect("failed to set event id");
        assert_eq!(
            tx.get_last_event_id(EVENT_TYPE_ID_1)
                .expect("failed to get event id"),
            Some(event_id2)
        );
        tx.set_last_event_id(EVENT_TYPE_ID_2, &event_id3)
            .expect("failed to set event id");
        assert_eq!(
            tx.get_last_event_id(EVENT_TYPE_ID_2)
                .expect("failed to get event id"),
            Some(event_id3)
        );
        tx.delete_last_event_id(EVENT_TYPE_ID_1)
            .expect("failed to delete event");
        assert!(tx
            .get_last_event_id(EVENT_TYPE_ID_1)
            .expect("failed to get event id")
            .is_none());
        tx.commit().await.expect("Failed to commit transaction");
}
