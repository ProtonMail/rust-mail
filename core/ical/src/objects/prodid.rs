use super::*;

/// Product identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.7.3>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProdId {
    pub value: Text,
}

impl<T> From<T> for ProdId
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self {
            value: Text::new(value),
        }
    }
}
