use crate::properties::anniversary::{validate_anniversary, Anniversary, AnniversaryValue};
use crate::test::{make_property, property_reject_parameters};
use crate::values::date_and_or_time::DateAndOrTimeValue;
use crate::values::text::Text;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn anniversary_struct() {
    let anniversary = Anniversary::new_validated("some text").unwrap();
    assert_eq!(
        anniversary.value,
        AnniversaryValue::Text(Text::new_unchecked("some text"))
    );
    let anniversary = Anniversary::new_validated("20001231T125959+0100").unwrap();
    assert_eq!(
        anniversary.value,
        AnniversaryValue::DateAndOrTime(
            DateAndOrTimeValue::new_validated("20001231T125959+0100").unwrap()
        )
    );
}

#[test]
fn anniversary_property() {
    validate_anniversary(&make_property("ANNIVERSARY", Some("T01"), None)).unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("T01"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("T01"),
        Some(vec![
            ("VALUE", vec!["date-and-or-time"]),
            ("ALTID", vec!["param-value"]),
            ("CALSCALE", vec!["gregorian"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_anniversary,
        "ANNIVERSARY",
        "T01",
        hash_set! {ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_anniversary,
        "ANNIVERSARY",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
