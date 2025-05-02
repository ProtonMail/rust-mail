use super::*;

/// Date (year, month, and day).
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    year: Year,
    month: Month,
    day: Day,
}

impl Date {
    #[must_use]
    pub fn new(year: Year, month: Month, day: Day) -> Self {
        Self { year, month, day }
    }

    #[must_use]
    pub fn new_unchecked(year: u16, month: u8, day: u8) -> Self {
        Self {
            year: Year::new_unchecked(year),
            month: Month::new_unchecked(month),
            day: Day::new_unchecked(day),
        }
    }

    #[must_use]
    pub fn year(&self) -> Year {
        self.year
    }

    #[must_use]
    pub fn month(&self) -> Month {
        self.month
    }

    #[must_use]
    pub fn day(&self) -> Day {
        self.day
    }
}
