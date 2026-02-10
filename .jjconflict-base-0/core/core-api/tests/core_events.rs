use proton_core_api::services::proton::CoreEvent;

#[test]
fn test_deserialize() {
    let event_json = include_str!("event.json");
    let mail_event: CoreEvent = serde_json::from_str(event_json).unwrap();

    insta::assert_debug_snapshot!(mail_event);
}
