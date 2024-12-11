use pretty_assertions::assert_eq;
use std::fs::{read_to_string, write};

use proton_api_mail::services::proton::response_data::MailEvent;

#[test]
fn test_deserialize() {
    let input = read_to_string("tests/data/mail_events/event.json").unwrap();
    let mail_event: MailEvent = serde_json::from_str(&input).unwrap();
    let actual = serde_json::to_string_pretty(&mail_event).unwrap();
    let expected = read_to_string("tests/data/mail_events/expected.json").unwrap();

    let actual = actual.trim();
    let expected = expected.trim();
    if actual != expected {
        write("tests/data/mail_events/expected.json.new", actual).unwrap();
    }

    assert_eq!(expected, actual);
}
