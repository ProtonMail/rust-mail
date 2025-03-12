use crate::ParameterType;
use crate::properties::birthday::{Birthday, BirthdayValue, validate_bday};
use crate::test::{make_property, property_reject_parameters};
use crate::values::date_and_or_time::DateAndOrTimeValue;
use crate::values::text::Text;
use velcro::hash_set;

#[test]
fn birthday_struct() {
    let birthday = Birthday::new_validated("some text").unwrap();
    assert_eq!(
        birthday.value,
        BirthdayValue::Text(Text::new_unchecked("some text"))
    );
    let birthday = Birthday::new_validated("20001231T125959+0100").unwrap();
    assert_eq!(
        birthday.value,
        BirthdayValue::DateAndOrTime(
            DateAndOrTimeValue::new_validated("20001231T125959+0100").unwrap()
        )
    );
}

#[test]
fn bday_property() {
    validate_bday(&make_property("BDAY", Some("T01"), None)).unwrap();
    validate_bday(&make_property(
        "BDAY",
        Some("T01"),
        Some(vec![
            ("VALUE", vec!["date-and-or-time"]),
            ("ALTID", vec!["param-value"]),
            ("CALSCALE", vec!["gregorian"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_bday(&make_property(
        "BDAY",
        Some("T"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_bday,
        "BDAY",
        "T01",
        hash_set! {ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_bday,
        "N",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
