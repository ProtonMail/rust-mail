#[macro_export]
/// Creates a `NonEmptyString` from a string literal at compile time.
///
/// This macro ensures that the provided string is **not empty** at compile time,
/// preventing runtime errors due to empty strings.
///
/// # Examples
///
/// ```
/// use non_empty_string::{non_empty_string, NonEmptyString};
///
/// let s: NonEmptyString = non_empty_string!("Hello, Rust!");
/// assert_eq!(s, NonEmptyString::new("Hello, Rust!".to_string()).unwrap());
/// ```
///
/// # Compile-time Failure
///
/// If an empty string is provided, this macro will cause a **compile-time error**.
///
/// ```compile_fail
/// use non_empty_string::non_empty_string;
///
/// let s = non_empty_string!("");
/// ```
macro_rules! non_empty_string {
    ($s:expr) => {{
        // Compile-time assertion to ensure the string is non-empty
        const _: () = assert!(!$s.is_empty(), "String cannot be empty");

        // Create a NonEmptyString, unsafely wrapping since we've checked it's valid
        unsafe { $crate::NonEmptyString::new_unchecked($s.to_string()) }
    }};
}

#[cfg(test)]
mod tests {
    // We explicitely DO NOT do `use crate::NonEmptyString` or anything of the sorts to ensure the macro has proper hygiene.
    // Otherwise tests might pass, but if a user does `non_empty_string::non_empty_string!("A")`, they might get compilation
    // errors that `NonEmptyString` is not in scope.

    const NON_EMPTY_STRING: &'static str = "non-empty-string";

    #[test]
    fn test_const_non_empty_string_macro_valid() {
        let s = non_empty_string!(NON_EMPTY_STRING);
        assert_eq!(
            s,
            crate::NonEmptyString::try_from(NON_EMPTY_STRING).unwrap()
        );
    }

    #[test]
    fn test_inline_non_empty_string_macro_valid() {
        let s = non_empty_string!("Test String");
        assert_eq!(s, crate::NonEmptyString::try_from("Test String").unwrap());
    }
}
