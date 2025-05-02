use super::*;

/// Recurrence rule.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.3.10>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recur {
    pub freq: Freq,
    pub until: Option<DateOrDt<UtcOrLocalForm>>,
    pub count: Option<u32>,
    pub interval: Option<u32>, // it's actually NonZeroU32, but that's not supported by ext-php-rs
    pub by_second: Vec<Second>,
    pub by_minute: Vec<Minute>,
    pub by_hour: Vec<Hour>,
    pub by_day: Vec<ByDay>,
    pub by_month_day: Vec<Signed<Day>>,
    pub by_year_day: Vec<Signed<DayOrdinal>>,
    pub by_week_no: Vec<Signed<WeekOrdinal>>,
    pub by_month: Vec<Month>,
    pub by_set_pos: Vec<Signed<DayOrdinal>>,
    pub wkst: Option<Weekday>,
}

impl Recur {
    #[must_use]
    pub fn new(freq: Freq) -> Self {
        Self {
            freq,
            until: None,
            count: None,
            interval: None,
            by_second: Vec::new(),
            by_minute: Vec::new(),
            by_hour: Vec::new(),
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_year_day: Vec::new(),
            by_week_no: Vec::new(),
            by_month: Vec::new(),
            by_set_pos: Vec::new(),
            wkst: None,
        }
    }

    #[must_use]
    pub fn with_until(mut self, until: impl Into<DateOrDt<UtcOrLocalForm>>) -> Self {
        self.until = Some(until.into());
        self
    }

    #[must_use]
    pub fn with_count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    #[must_use]
    pub fn with_interval(mut self, interval: u32) -> Self {
        self.interval = Some(interval);
        self
    }

    #[must_use]
    pub fn with_by_second(mut self, by_second: impl IntoIterator<Item = Second>) -> Self {
        self.by_second = by_second.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_minute(mut self, by_minute: impl IntoIterator<Item = Minute>) -> Self {
        self.by_minute = by_minute.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_hour(mut self, by_hour: impl IntoIterator<Item = Hour>) -> Self {
        self.by_hour = by_hour.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_day(mut self, by_day: impl IntoIterator<Item = ByDay>) -> Self {
        self.by_day = by_day.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_month_day(
        mut self,
        by_month_day: impl IntoIterator<Item = Signed<Day>>,
    ) -> Self {
        self.by_month_day = by_month_day.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_year_day(
        mut self,
        by_year_day: impl IntoIterator<Item = Signed<DayOrdinal>>,
    ) -> Self {
        self.by_year_day = by_year_day.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_week_no(
        mut self,
        by_week_no: impl IntoIterator<Item = Signed<WeekOrdinal>>,
    ) -> Self {
        self.by_week_no = by_week_no.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_month(mut self, by_month: impl IntoIterator<Item = Month>) -> Self {
        self.by_month = by_month.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_by_set_pos(
        mut self,
        by_set_pos: impl IntoIterator<Item = Signed<DayOrdinal>>,
    ) -> Self {
        self.by_set_pos = by_set_pos.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_wkst(mut self, wkst: Weekday) -> Self {
        self.wkst = Some(wkst);
        self
    }
}

/// Recurrence rule's frequency; see [`Recur`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freq {
    Secondly,
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByDay {
    /// E.g. `MO`
    Every(Weekday),

    /// E.g. `1TU`, `-2WE`
    Specific(NonZeroI8, Weekday),
}
