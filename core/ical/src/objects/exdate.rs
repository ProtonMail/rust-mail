use super::*;

/// Exception date-times.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.5.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExDate {
    Dates(Vec<Date>),
    DateTimes(AnyForm, Vec<(Date, Time)>),
}
