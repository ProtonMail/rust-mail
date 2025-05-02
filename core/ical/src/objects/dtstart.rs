use super::*;

/// Date-time start.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.2.4>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtStart {
    pub value: DateOrDt,
}

impl<T> From<T> for DtStart
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
