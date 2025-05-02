use super::*;

/// Description.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.5>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub value: Text,
}

impl<T> From<T> for Description
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
