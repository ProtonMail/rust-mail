use super::*;

/// Unique identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.4.7>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uid {
    pub value: Text,
}

impl<T> From<T> for Uid
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
