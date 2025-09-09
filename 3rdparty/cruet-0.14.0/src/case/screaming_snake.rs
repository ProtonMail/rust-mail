use crate::case::*;
/// Converts a `&str` to `SCREAMING_SNAKE_CASE` `String`
///
/// ```
/// use cruet::case::to_screaming_snake_case;
///
/// assert_eq!(to_screaming_snake_case("foo_bar"), "FOO_BAR");
/// assert_eq!(to_screaming_snake_case("HTTP Foo bar"), "HTTP_FOO_BAR");
/// assert_eq!(to_screaming_snake_case("Foo bar"), "FOO_BAR");
/// assert_eq!(to_screaming_snake_case("Foo Bar"), "FOO_BAR");
/// assert_eq!(to_screaming_snake_case("FooBar"), "FOO_BAR");
/// assert_eq!(to_screaming_snake_case("fooBar"), "FOO_BAR");
/// assert_eq!(to_screaming_snake_case("fooBar3"), "FOO_BAR_3");
/// ```
pub fn to_screaming_snake_case(non_snake_case_string: &str) -> String {
    to_case_snake_like(non_snake_case_string, "_", "upper")
}

/// Determines of a `&str` is `SCREAMING_SNAKE_CASE`
///
/// ```
/// use cruet::case::is_screaming_snake_case;
///
/// assert!(is_screaming_snake_case("FOO_BAR_STRING_THAT_IS_REALLY_REALLY_LONG"));
/// assert!(is_screaming_snake_case("FOO_BAR1_STRING_THAT_IS_REALLY_REALLY_LONG"));
/// assert!(is_screaming_snake_case("FOO_BAR_1_STRING_THAT_IS_REALLY_REALLY_LONG"));
///
/// assert!(!is_screaming_snake_case("Foo bar string that is really really long"));
/// assert!(!is_screaming_snake_case("foo-bar-string-that-is-really-really-long"));
/// assert!(!is_screaming_snake_case("FooBarIsAReallyReallyLongString"));
/// assert!(!is_screaming_snake_case("Foo Bar Is A Really Really Long String"));
/// assert!(!is_screaming_snake_case("fooBarIsAReallyReallyLongString"));
/// ```
pub fn is_screaming_snake_case(test_string: &str) -> bool {
    test_string == to_screaming_snake_case(test_string)
}

#[cfg(all(feature = "unstable", test))]
mod benchmarks {
    extern crate test;
    use self::test::Bencher;

    #[bench]
    fn bench_screaming_snake(b: &mut Bencher) {
        b.iter(|| super::to_screaming_snake_case("Foo bar"));
    }

    #[bench]
    fn bench_is_screaming_snake(b: &mut Bencher) {
        b.iter(|| super::is_screaming_snake_case("Foo bar"));
    }
}

#[cfg(test)]
mod tests {
    use super::is_screaming_snake_case;
    use super::to_screaming_snake_case;

    #[test]
    fn from_camel_case() {
        let convertable_string: String = "fooBar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_pascal_case() {
        let convertable_string: String = "FooBar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_kebab_case() {
        let convertable_string: String = "foo-bar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_sentence_case() {
        let convertable_string: String = "Foo bar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_title_case() {
        let convertable_string: String = "Foo Bar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_train_case() {
        let convertable_string: String = "Foo-Bar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_screaming_screaming_snake_case() {
        let convertable_string: String = "FOO_BAR".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn from_snake_case() {
        let convertable_string: String = "foo_bar".to_owned();
        let expected: String = "FOO_BAR".to_owned();
        assert_eq!(to_screaming_snake_case(&convertable_string), expected)
    }

    #[test]
    fn is_correct_from_camel_case() {
        let convertable_string: String = "fooBar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_pascal_case() {
        let convertable_string: String = "FooBar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_kebab_case() {
        let convertable_string: String = "foo-bar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_sentence_case() {
        let convertable_string: String = "Foo bar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_title_case() {
        let convertable_string: String = "Foo Bar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_train_case() {
        let convertable_string: String = "Foo-Bar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }

    #[test]
    fn is_correct_from_screaming_screaming_snake_case() {
        let convertable_string: String = "FOO_BAR".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), true)
    }

    #[test]
    fn is_correct_from_snake_case() {
        let convertable_string: String = "foo_bar".to_owned();
        assert_eq!(is_screaming_snake_case(&convertable_string), false)
    }
}
