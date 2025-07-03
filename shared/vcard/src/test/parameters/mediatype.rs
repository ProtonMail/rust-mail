use crate::parameters::mediatype::MediaType;
use crate::parameters::mediatype::is_mediatype_param;

#[test]
fn mediatype_struct() {
    assert!(MediaType::new_validated("type/subtype;parameter=value;parameter=value").is_ok());
    assert!(MediaType::new_validated("foo").is_err());
}

#[test]
fn mediatype_param() {
    assert!(is_mediatype_param(&[
        "AZaz09!#$&.+-^_/AZaz09!#$&.+-^_".to_owned()
    ]));
    assert!(is_mediatype_param(&[
        "AZaz09!#$&.+-^_/AZaz09!#$&.+-^_;foo=bar".to_owned()
    ]));
    assert!(is_mediatype_param(&[
        "AZaz09!#$&.+-^_/AZaz09!#$&.+-^_;foo=bar;caz=toto".to_owned()
    ]));
    assert!(is_mediatype_param(&[
        r#"AZaz09!#$&.+-^_/AZaz09!#$&.+-^_;foo="bar bar";caz="toto tutu""#.to_owned()
    ]));
    assert!(!is_mediatype_param(&[]));
    assert!(!is_mediatype_param(&[
        "AZaz09!#$&.+-^_/AZaz09!#$&.+-^_".to_owned(),
        "AZaz09!#$&.+-^_/AZaz09!#$&.+-^_".to_owned(),
    ]));
    assert!(is_mediatype_param(&["text/calendar".to_owned()]));
}
