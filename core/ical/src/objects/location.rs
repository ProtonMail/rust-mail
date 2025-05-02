use super::*;

/// Location.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.7>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub value: Text,
}

impl<T> From<T> for Location
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
