use super::*;

/// Date, time, and the associated form (describing the time zone etc.).
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime<F = AnyForm> {
    pub date: Date,
    pub time: Time,
    pub form: F,
}
