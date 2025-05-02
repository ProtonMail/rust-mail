use super::*;

/// Summary.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.12>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub value: Text,
}

impl<T> From<T> for Summary
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
