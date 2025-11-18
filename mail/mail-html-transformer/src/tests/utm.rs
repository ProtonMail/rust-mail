use super::*;

#[test]
fn remove_from_url() {
    let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example&UserID=123";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.0.as_str(), "https://example.com/?UserID=123");

    let url = "panda"; // Invalid URL
    let new_url = strip_from_string(url);
    assert!(new_url.is_err());
}

#[test]
fn preserve_params_without_values() {
    let url = "https://example.com?foo&bar=1";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.0.as_str(), "https://example.com/?foo&bar=1");
}
