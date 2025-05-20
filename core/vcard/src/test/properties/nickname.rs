use velcro::hash_set;

use crate::ParameterType;
use crate::properties::nickname::{Nickname, validate_nickname};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text_list::TextList;

#[test]
fn nickname_struct() {
    let nickname = Nickname {
        value: "a,b,c".into(),
        ..Default::default()
    };
    assert_eq!(nickname.value, TextList::from("a,b,c"));
}

#[test]
fn nickname_property() {
    validate_nickname(&make_property("NICKNAME", Some("a,b,c"), None)).unwrap();
    validate_nickname(&make_property(
        "NICKNAME",
        Some("a"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("TYPE", vec!["work", "home"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_nickname,
        "NICKNAME",
        "a,b,c",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
