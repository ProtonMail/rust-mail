use velcro::hash_set;

use crate::properties::photo::{validate_photo, Photo};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn photo_struct() {
    let photo = Photo::new_validated("uri:uri").unwrap();
    assert_eq!(photo.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn photo_property() {
    validate_photo(&make_property(
        "PHOTO",
        Some("ftp://ftp.is.co.za/rfc/rfc1808.txt"),
        None,
    ))
    .unwrap();
    validate_photo(&make_property(
        "PHOTO",
        Some("url:url"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("ALTID", vec!["param-value"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("PREF", vec!["1"]),
            ("PID", vec!["1.2", "3.4"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_photo,
        "PHOTO",
        "url:url",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
