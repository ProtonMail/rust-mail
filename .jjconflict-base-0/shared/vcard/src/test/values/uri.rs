use crate::values::uri::Uri;
use url::Url;

#[test]
fn uri_struct() {
    let uri = Uri::new_validated("tel:+1-816-555-1212").unwrap();
    assert_eq!(uri.0, Url::parse("tel:+1-816-555-1212").unwrap());
}
