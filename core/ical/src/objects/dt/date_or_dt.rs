use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrDt<F = AnyForm> {
    Date(Date),
    DateTime(DateTime<F>),
}

impl From<Date> for DateOrDt {
    fn from(value: Date) -> Self {
        DateOrDt::Date(value)
    }
}

impl<F> From<DateTime<F>> for DateOrDt<F> {
    fn from(value: DateTime<F>) -> Self {
        DateOrDt::DateTime(value)
    }
}
