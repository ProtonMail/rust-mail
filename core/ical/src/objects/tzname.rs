use super::*;

/// Time zone name.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TzName {
    pub value: Text,
}

impl<T> From<T> for TzName
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
