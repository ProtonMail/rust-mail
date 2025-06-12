use super::*;

/// Duration.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Property> for Duration {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;
        r.value()
    }
}

impl IcsWrite<Property> for Duration {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(self);
    }
}

impl IcsRead<Value> for Duration {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self {
            sign: r.value()?,
            amount: r.value()?,
        })
    }
}

impl IcsWrite<Value> for Duration {
    fn write(&self, w: &mut IcsWriter) {
        if self.is_zero() {
            w.raw("P0D");
            return;
        }

        match self.sign {
            Sign::Pos => {
                // Duration is implied to be positive, no sign required
            }
            Sign::Neg => {
                w.raw("-");
            }
        }

        w.value(self.amount);
    }
}

/// Duration's amount, see [`Duration`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Value> for DurationAmount {
    fn read(r: &mut IcsReader) -> Option<Self> {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Part {
            DateOrWeek,
            Time,
        }

        let mut part = Part::DateOrWeek;
        let mut weeks = 0;
        let mut days = 0;
        let mut hours = 0;
        let mut minutes = 0;
        let mut seconds = 0;

        r.eat('P')?;

        while !matches!(r.peek(), Some('\n' | ',') | None) {
            if r.try_eat('T').is_some() {
                part = Part::Time;
            }

            let amount = r.value()?;
            let unit = r.value::<Spanned<_>>()?;

            match (part, unit.value) {
                (Part::DateOrWeek, 'w' | 'W') => weeks = amount,
                (Part::DateOrWeek, 'd' | 'D') => days = amount,
                (Part::Time, 'h' | 'H') => hours = amount,
                (Part::Time, 'm' | 'M') => minutes = amount,
                (Part::Time, 's' | 'S') => seconds = amount,

                _ => {
                    r.error(unit.span, format!("unknown duration unit `{}`", unit.value));
                }
            }
        }

        if weeks > 0 {
            for (unit, amount) in [('D', days), ('H', hours), ('M', minutes), ('S', seconds)] {
                if amount > 0 {
                    r.error(
                        None,
                        format!("duration unit `{unit}` is not supported together with `W`"),
                    );
                }
            }

            return Some(DurationAmount::Week(WeekDuration { weeks }));
        }

        if days > 0 {
            return Some(DurationAmount::Date(DateDuration {
                days,
                time: TimeDuration {
                    hours,
                    minutes,
                    seconds,
                },
            }));
        }

        Some(DurationAmount::Time(TimeDuration {
            hours,
            minutes,
            seconds,
        }))
    }
}

impl IcsWrite<Value> for DurationAmount {
    fn write(&self, w: &mut IcsWriter) {
        w.raw("P");

        match self {
            DurationAmount::Date(this) => w.value(this),
            DurationAmount::Time(this) => w.value(this),
            DurationAmount::Week(this) => w.value(this),
        }
    }
}

/// Date and time part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsWrite<Value> for DateDuration {
    fn write(&self, w: &mut IcsWriter) {
        if self.days > 0 {
            w.value(self.days);
            w.raw("D");
        }

        if !self.time.is_zero() {
            w.value(self.time);
        }
    }
}

/// Time part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsWrite<Value> for TimeDuration {
    fn write(&self, w: &mut IcsWriter) {
        w.raw("T");

        if self.hours > 0 {
            w.value(self.hours);
            w.raw("H");
        }

        if self.minutes > 0 {
            w.value(self.minutes);
            w.raw("M");
        }

        if self.seconds > 0 {
            w.value(self.seconds);
            w.raw("S");
        }
    }
}

/// Week part of a [`Duration`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.6>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsWrite<Value> for WeekDuration {
    fn write(&self, w: &mut IcsWriter) {
        if self.weeks > 0 {
            w.value(self.weeks);
            w.raw("W");
        }
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
        assert_eq!("P1DT2H3M4S", target.to_string(Value));
        assert_eq!(":P1DT2H3M4S", target.to_string(Property));
        assert_trip!("P1DT2H3M4S", Duration as Value);
        assert_trip!(":P1DT2H3M4S", Duration as Property);
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
        assert_eq!("PT1H2M3S", target.to_string(Value));
        assert_eq!(":PT1H2M3S", target.to_string(Property));
        assert_trip!("PT1H2M3S", Duration as Value);
        assert_trip!(":PT1H2M3S", Duration as Property);
    }

    #[test]
    fn week() {
        let target = WeekDuration::new(1);

        assert!(!target.is_zero());
        assert_eq!(1, target.weeks);

        let target = Duration::pos(target);

        assert!(!target.is_zero());
        assert_eq!("P1W", target.to_string(Value));
        assert_eq!(":P1W", target.to_string(Property));
        assert_trip!("P1W", Duration as Value);
        assert_trip!(":P1W", Duration as Property);
    }

    #[test]
    fn negative() {
        let target = TimeDuration::new(10, 20, 30);

        assert!(!target.is_zero());
        assert_eq!("PT10H20M30S", Duration::pos(target).to_string(Value));
        assert_eq!(":PT10H20M30S", Duration::pos(target).to_string(Property));
        assert_eq!("-PT10H20M30S", Duration::neg(target).to_string(Value));
        assert_eq!(":-PT10H20M30S", Duration::neg(target).to_string(Property));
        assert_trip!("PT10H20M30S", Duration as Value);
        assert_trip!(":PT10H20M30S", Duration as Property);
        assert_trip!("-PT10H20M30S", Duration as Value);
        assert_trip!(":-PT10H20M30S", Duration as Property);
    }

    #[test]
    fn zero() {
        let target = Duration::default();

        assert!(target.is_zero());
        assert_eq!("P0D", target.to_string(Value));
        assert_eq!(":P0D", target.to_string(Property));
        assert_trip!("P0D", Duration as Value);
        assert_trip!(":P0D", Duration as Property);

        // ---

        let date = DurationAmount::Date(DateDuration::new(0, TimeDuration::new(0, 0, 0)));
        let time = DurationAmount::Time(TimeDuration::new(0, 0, 0));
        let week = DurationAmount::Week(WeekDuration::new(0));

        for sign in [Sign::Pos, Sign::Neg] {
            for amount in [date, time, week] {
                let target = Duration::new(sign, amount);

                assert!(amount.is_zero());
                assert!(target.is_zero());
                assert_eq!("P0D", target.to_string(Value));
                assert_eq!(":P0D", target.to_string(Property));
            }
        }
    }

    #[test]
    fn unsupported_combination() {
        assert_trip!(
            "P2W3D" => "P2W", yielding [
                ReadMsg {
                    at: None,
                    body: "duration unit `D` is not supported together with `W`".into(),
                    kind: ReadMsgKind::Error,
                    context: vec![
                        Spanned {
                            span: Span::new((1, 1), (1, 1)),
                            value: "`DurationAmount`".into(),
                        },
                    ],
                },
            ],
            Duration as Value
        );
    }
}
