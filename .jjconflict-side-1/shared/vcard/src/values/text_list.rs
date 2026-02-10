use std::fmt::Debug;

use crate::values::text::Text;
use crate::vcard::split_list;

/// Represent a text-list value from vCard RFC6350
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextList(pub Vec<Text>);

impl TextList {
    /// Create a new text-list from given values (no check are done)
    #[must_use]
    pub fn new(value: &[String]) -> Self {
        Self(value.iter().map(Into::into).collect())
    }
}

impl<T: AsRef<str>> From<T> for TextList {
    fn from(value: T) -> Self {
        let values = split_list(value.as_ref(), ',')
            .iter()
            .map(Into::into)
            .collect();
        Self(values)
    }
}
