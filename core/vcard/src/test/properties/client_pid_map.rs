use crate::properties::client_pid_map::{ClientPidMap, validate_clientpidmap};
use crate::test::make_property;
use crate::values::uri::Uri;

#[test]
fn client_pid_map_struct() {
    let client_pid_map = ClientPidMap::new_validated("123;uri:uri").unwrap();
    assert_eq!(client_pid_map.index, 123);
    assert_eq!(client_pid_map.uri, Uri::new("uri:uri".parse().unwrap()));
}

#[test]
fn clientpidmap_property() {
    validate_clientpidmap(&make_property("CLIENTPIDMAP", Some("123;uri:uri"), None)).unwrap();
    validate_clientpidmap(&make_property(
        "CLIENTPIDMAP",
        Some("123;uri:uri"),
        Some(vec![("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
}
