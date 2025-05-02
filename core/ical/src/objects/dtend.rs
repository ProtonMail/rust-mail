use super::*;

/// Date-time end.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtEnd {
    pub value: DateOrDt,
}

impl<T> From<T> for DtEnd
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
