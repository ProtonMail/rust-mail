use proton_mail_api::services::proton::response_data::MailEvent;

#[test]
fn test_deserialize() {
    let event_json = include_str!("event.json");
    let mail_event: MailEvent = serde_json::from_str(event_json).unwrap();

    insta::assert_debug_snapshot!(mail_event);
}
