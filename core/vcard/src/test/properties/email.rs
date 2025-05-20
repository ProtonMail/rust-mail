use velcro::hash_set;

use crate::ParameterType;
use crate::properties::email::validate_email;
use crate::test::{make_property, property_reject_parameters};

#[test]
fn email_property() {
    validate_email(&make_property("EMAIL", Some("text"), None)).unwrap();
    validate_email(&make_property(
        "EMAIL",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("PID", vec!["1.2", "2.3"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_email,
        "EMAIL",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
