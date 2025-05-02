use super::*;

/// Recurrence id.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurrenceId {
    pub value: DateOrDt,
}

impl<T> From<T> for RecurrenceId
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
