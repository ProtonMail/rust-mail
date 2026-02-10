use super::*;

#[test]
fn remove_from_url() {
    let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example&UserID=123";
    let new_url = strip_from_string(url).unwrap().unwrap();
    assert_eq!(new_url.as_str(), "https://example.com/?UserID=123");

    let url = "panda"; // Invalid URL
    let new_url = strip_from_string(url);
    assert!(new_url.is_err());
}

#[test]
fn preserve_params_without_values() {
    let url = "https://example.com?foo&bar=1";
    let new_url = strip_from_string(url).unwrap();
    assert!(new_url.is_none());

    let url = "https://example.com?foo&bar=1&utm_source=example";
    let new_url = strip_from_string(url).unwrap().unwrap();
    assert_eq!(new_url.as_str(), "https://example.com/?foo&bar=1");
}

#[test]
fn test_transformer_utm() {
    let body = r#"
        <a href="https://example.com?foo=1">Example</a>
        <a href="https://example.com/?utm_source=example&utm_medium=example&utm_campaign=example">Tracker Example</a>
    "#;

    let mut transformer = crate::Transformer::new(body);
    let results = transformer.strip_utm();
    let body = transformer.extract_body();

    insta::assert_snapshot!(body);
    insta::assert_debug_snapshot!(results);
}

#[test]
fn url_encodable_query_without_utm_returns_none() {
    let url = "https://example.com/redirect?data=eyJtaWQiOiJhYmMxMjM0NTY3ODkwIiwiY3QiOiJ0ZXN0LWRlNjcxNzM0Y2ExYTExZGMzOWUwOTkwODI0YmNjMTExLTEyMzQiLCJyZCI6ImV4YW1wbGUuY29tIn0/VaHR0cHM6Ly93d3cuZXhhbXBsZS5jb20/SWkhfRXhhbXBsZUFsZXJ0c19OREJBTA/LY24y/qP3NvdXJjZT1FeGFtcGxlK0JyZWFraW5nK05ld3MmbWVkaXVtPWVtYWlsJmJ0X2VlPVNIeTJvODA0JTJGMzJaRDBhSHhmcmNYTiUyQklvSXltbyUyRnZEemxnTklyWW42ekF1SUNmMUxCR0taeA/gaWrMzw/JMDEyMzQ1NkMxMjM0NTZCMTM1Mg/s9r35be1eaa";
    let result = strip_from_string(url).unwrap();
    assert!(result.is_none());
}
