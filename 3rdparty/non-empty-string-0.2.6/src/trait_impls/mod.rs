use std::borrow::Borrow;

mod delegated_traits;

use crate::NonEmptyString;

#[cfg(not(no_global_oom_handling))]
impl From<char> for NonEmptyString {
    #[inline]
    fn from(c: char) -> Self {
        let string = c.to_string();
        NonEmptyString::new(string).expect("since there is a singular char, the string will not be empty as it will contain exactly one char")
    }
}

// Defined in the file for [`str`], not [`String`]
impl Borrow<str> for NonEmptyString {
    fn borrow(&self) -> &str {
        <String as Borrow<str>>::borrow(&self.0)
    }
}

impl Borrow<String> for NonEmptyString {
    fn borrow(&self) -> &String {
        &self.0
    }
}
