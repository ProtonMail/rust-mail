use super::*;

/// Time (hour, minute, and second).
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: Hour,
    pub minute: Minute,
    pub second: Second,
}

impl Time {
    #[must_use]
    pub fn new_unchecked(hour: u8, minute: u8, second: u8) -> Self {
        Self {
            hour: Hour::new_unchecked(hour),
            minute: Minute::new_unchecked(minute),
            second: Second::new_unchecked(second),
        }
    }
}
