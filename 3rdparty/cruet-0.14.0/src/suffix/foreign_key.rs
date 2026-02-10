use crate::case::snake::to_snake_case;

/// Converts a `&str` to a `foreign_key`
///
/// ```
/// use cruet::suffix::foreign_key::to_foreign_key;
///
/// assert!(to_foreign_key("foo_bar") == "foo_bar_id");
/// assert!(to_foreign_key("Foo bar") == "foo_bar_id");
/// assert!(to_foreign_key("Foo Bar") == "foo_bar_id");
/// assert!(to_foreign_key("Foo::Bar") == "bar_id");
/// assert!(to_foreign_key("Test::Foo::Bar") == "bar_id");
/// assert!(to_foreign_key("FooBar") == "foo_bar_id");
/// assert!(to_foreign_key("fooBar") == "foo_bar_id");
/// assert!(to_foreign_key("fooBar3") == "foo_bar_3_id");
/// ```
pub fn to_foreign_key(non_foreign_key_string: &str) -> String {
    if non_foreign_key_string.contains("::") {
        let split_string: Vec<&str> = non_foreign_key_string.split("::").collect();
        safe_convert(split_string[split_string.len() - 1])
    } else {
        safe_convert(non_foreign_key_string)
    }
}
fn safe_convert(safe_string: &str) -> String {
    let snake_cased: String = to_snake_case(safe_string);
    if snake_cased.ends_with("_id") {
        snake_cased
    } else {
        format!("{}{}", snake_cased, "_id")
    }
}

/// Determines if a `&str` is a `foreign_key`
///
/// ```
/// use cruet::suffix::foreign_key::is_foreign_key;
///
/// assert!(!is_foreign_key("Foo bar string that is really really long"));
/// assert!(!is_foreign_key("foo-bar-string-that-is-really-really-long"));
/// assert!(!is_foreign_key("FooBarIsAReallyReallyLongString"));
/// assert!(!is_foreign_key("Foo Bar Is A Really Really Long String"));
/// assert!(!is_foreign_key("fooBarIsAReallyReallyLongString"));
/// assert!(!is_foreign_key("foo_bar_string_that_is_really_really_long"));
/// assert!(is_foreign_key("foo_bar_string_that_is_really_really_long_id"));
/// ```
pub fn is_foreign_key(test_string: &str) -> bool {
    to_foreign_key(test_string) == test_string
}
