use super::*;

/// Duration.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duration {
    pub sign: Sign,
    pub amount: DurationAmount,
}

impl Duration {
    #[must_use]
    pub fn new(sign: Sign, amount: impl Into<DurationAmount>) -> Self {
        Self {
            sign,
            amount: amount.into(),
        }
    }

    #[must_use]
    pub fn neg(amount: impl Into<DurationAmount>) -> Self {
        Self::new(Sign::Neg, amount)
    }

    #[must_use]
    pub fn pos(amount: impl Into<DurationAmount>) -> Self {
        Self::new(Sign::Pos, amount)
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.amount.is_zero()
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self {
            sign: Sign::Pos,
            amount: DurationAmount::default(),
        }
    }
}

/// Duration's amount, see [`Duration`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationAmount {
    Date(DateDuration),
    Time(TimeDuration),
    Week(WeekDuration),
}

impl DurationAmount {
    #[must_use]
    pub fn is_zero(&self) -> bool {
        match self {
            DurationAmount::Date(this) => this.is_zero(),
            DurationAmount::Time(this) => this.is_zero(),
            DurationAmount::Week(this) => this.is_zero(),
        }
    }
}

impl Default for DurationAmount {
    fn default() -> Self {
        DurationAmount::Date(DateDuration::default())
    }
}

impl From<DateDuration> for DurationAmount {
    fn from(dur: DateDuration) -> Self {
        DurationAmount::Date(dur)
    }
}

impl From<TimeDuration> for DurationAmount {
    fn from(dur: TimeDuration) -> Self {
        DurationAmount::Time(dur)
    }
}

impl From<WeekDuration> for DurationAmount {
    fn from(dur: WeekDuration) -> Self {
        DurationAmount::Week(dur)
    }
}

/// Date and time part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateDuration {
    pub days: u32,
    pub time: TimeDuration,
}

impl DateDuration {
    #[must_use]
    pub fn new(days: u32, time: TimeDuration) -> Self {
        Self { days, time }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// Time part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeDuration {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

impl TimeDuration {
    #[must_use]
    pub fn new(hours: u32, minutes: u32, seconds: u32) -> Self {
        Self {
            hours,
            minutes,
            seconds,
        }
    }

    #[must_use]
    pub fn hours(hours: u32) -> Self {
        Self::new(hours, 0, 0)
    }

    #[must_use]
    pub fn minutes(minutes: u32) -> Self {
        Self::new(0, minutes, 0)
    }

    #[must_use]
    pub fn seconds(seconds: u32) -> Self {
        Self::new(0, 0, seconds)
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// Week part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeekDuration {
    pub weeks: u32,
}

impl WeekDuration {
    #[must_use]
    pub fn new(weeks: u32) -> Self {
        Self { weeks }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date() {
        let target = DateDuration::new(1, TimeDuration::new(2, 3, 4));

        assert!(!target.is_zero());
        assert_eq!(1, target.days);
        assert_eq!(2, target.time.hours);
        assert_eq!(3, target.time.minutes);
        assert_eq!(4, target.time.seconds);

        let target = Duration::pos(target);

        assert!(!target.is_zero());
    }

    #[test]
    fn time() {
        let target = TimeDuration::new(1, 2, 3);

        assert!(!target.is_zero());
        assert_eq!(1, target.hours);
        assert_eq!(2, target.minutes);
        assert_eq!(3, target.seconds);

        let target = Duration::pos(target);

        assert!(!target.is_zero());
    }

    #[test]
    fn week() {
        let target = WeekDuration::new(1);

        assert!(!target.is_zero());
        assert_eq!(1, target.weeks);

        let target = Duration::pos(target);

        assert!(!target.is_zero());
    }

    #[test]
    fn zero() {
        let target = Duration::default();

        assert!(target.is_zero());

        // ---

        let date = DurationAmount::Date(DateDuration::new(0, TimeDuration::new(0, 0, 0)));
        let time = DurationAmount::Time(TimeDuration::new(0, 0, 0));
        let week = DurationAmount::Week(WeekDuration::new(0));

        for sign in [Sign::Pos, Sign::Neg] {
            for amount in [date, time, week] {
                let target = Duration::new(sign, amount);

                assert!(amount.is_zero());
                assert!(target.is_zero());
            }
        }
    }
}
