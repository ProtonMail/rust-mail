use crate::parameters::geo_localisation::{GeoLocalisation, is_geo_param};

#[test]
fn geo_localisation_struct() {
    assert!(GeoLocalisation::new_validated("geo:37.386013,-122.082932").is_ok());
    assert!(GeoLocalisation::new_validated("foo").is_err());
}

#[test]
fn geo_param() {
    assert!(is_geo_param(&[
        "ftp://ftp.is.co.za/rfc/rfc1808.txt".to_owned()
    ]));
    assert!(!is_geo_param(&["foo".to_owned()]));
    assert!(!is_geo_param(&[]));
    assert!(!is_geo_param(&[
        "ftp://ftp.is.co.za/rfc/rfc1808.txt".to_owned(),
        "http://www.ietf.org/rfc/rfc2396.txt".to_owned()
    ]));
}
