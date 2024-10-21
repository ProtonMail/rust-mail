use velcro::hash_set;

use crate::properties::nickname::{validate_nickname, Nickname};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text_list::TextList;
use crate::ParameterType;

#[test]
fn nickname_struct() {
    let nickname = Nickname::new_validated("a,b,c").unwrap();
    assert_eq!(nickname.value, TextList::try_from("a,b,c").unwrap());
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
