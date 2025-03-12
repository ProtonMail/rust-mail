use crate::values::uri::{Uri, is_uri_value};
use url::Url;

#[test]
fn uri_struct() {
    let uri = Uri::new_validated("tel:+1-816-555-1212").unwrap();
    assert_eq!(uri.0, Url::parse("tel:+1-816-555-1212").unwrap());
}

#[test]
fn uri_value() {
    assert!(is_uri_value("ftp://ftp.is.co.za/rfc/rfc1808.txt"));
    assert!(is_uri_value("http://www.ietf.org/rfc/rfc2396.txt"));
    assert!(is_uri_value("ldap://[2001:db8::7]/c=GB?objectClass?one"));
    assert!(is_uri_value("mailto:John.Doe@example.com"));
    assert!(is_uri_value("news:comp.infosystems.www.servers.unix"));
    assert!(is_uri_value("tel:+1-816-555-1212"));
    assert!(is_uri_value("telnet://192.0.2.16:80/"));
    assert!(is_uri_value(
        "urn:oasis:names:specification:docbook:dtd:xml:4.1.2"
    ));
    assert!(!is_uri_value(""));
    assert!(!is_uri_value("x-09-azAZ"));
}
