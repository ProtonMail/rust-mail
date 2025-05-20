use crate::ParameterType;
use crate::properties::gender::{Gender, GenderValue, validate_gender};
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn gender_struct() {
    let gender = Gender::new_validated("").unwrap();
    assert_eq!(gender.value, GenderValue::None(String::new()));
    let gender = Gender::new_validated("m").unwrap();
    assert_eq!(gender.value, GenderValue::Male(String::new()));
    let gender = Gender::new_validated("M").unwrap();
    assert_eq!(gender.value, GenderValue::Male(String::new()));
    let gender = Gender::new_validated("f").unwrap();
    assert_eq!(gender.value, GenderValue::Female(String::new()));
    let gender = Gender::new_validated("F").unwrap();
    assert_eq!(gender.value, GenderValue::Female(String::new()));
    let gender = Gender::new_validated("o").unwrap();
    assert_eq!(gender.value, GenderValue::Other(String::new()));
    let gender = Gender::new_validated("O").unwrap();
    assert_eq!(gender.value, GenderValue::Other(String::new()));
    let gender = Gender::new_validated("n").unwrap();
    assert_eq!(gender.value, GenderValue::NotApplicable(String::new()));
    let gender = Gender::new_validated("N").unwrap();
    assert_eq!(gender.value, GenderValue::NotApplicable(String::new()));
    let gender = Gender::new_validated("u").unwrap();
    assert_eq!(gender.value, GenderValue::Unknown(String::new()));
    let gender = Gender::new_validated("U").unwrap();
    assert_eq!(gender.value, GenderValue::Unknown(String::new()));
    let gender = Gender::new_validated(";it's complicated").unwrap();
    assert_eq!(
        gender.value,
        GenderValue::None("it's complicated".to_owned())
    );
}

#[test]
fn gender_property() {
    validate_gender(&make_property("GENDER", Some(""), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("m"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("f"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("o"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("n"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("u"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("M"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("F"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("O"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("N"), None)).unwrap();
    validate_gender(&make_property("GENDER", Some("U"), None)).unwrap();
    assert!(validate_gender(&make_property("GENDER", Some("X"), None)).is_err());
    validate_gender(&make_property(
        "GENDER",
        Some(""),
        Some(vec![("VALUE", vec!["text"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_gender,
        "GENDER",
        "U",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
