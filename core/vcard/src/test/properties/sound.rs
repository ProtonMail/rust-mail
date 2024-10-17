use velcro::hash_set;

use crate::properties::sound::{validate_sound, Sound};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn sound_struct() {
    let sound = Sound::new_validated("uri:uri").unwrap();
    assert_eq!(sound.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn sound_property() {
    validate_sound(&make_property("SOUND", Some("uri:uri"), None)).unwrap();
    validate_sound(&make_property(
        "SOUND",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_sound,
        "SOUND",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::SortAs, ParameterType::TZ},
    );
}
