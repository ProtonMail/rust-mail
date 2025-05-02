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

    pub(crate) fn validate(&self) -> Vec<RecurViolation> {
        let mut viols = Vec::new();

        if self.interval == Some(0) {
            viols.push(RecurViolation::ZeroInterval);
        }

        viols
    }
}

impl Read<Value> for Recur {
    fn read(r: &mut Reader) -> Option<Self> {
        /// Recovers reader by skipping to the next recurrence part.
        fn recover<T>(r: &mut Reader) -> T
        where
            T: Default,
        {
            loop {
                if matches!(r.peek(), Some(';' | '\n') | None) {
                    return T::default();
                }

                _ = r.char();
            }
        }

        let pos = r.pos();
        let mut freq = None;
        let mut until = None;
        let mut count = None;
        let mut interval = None;
        let mut by_second = Vec::new();
        let mut by_minute = Vec::new();
        let mut by_hour = Vec::new();
        let mut by_day = Vec::new();
        let mut by_month_day = Vec::new();
        let mut by_year_day = Vec::new();
        let mut by_week_no = Vec::new();
        let mut by_month = Vec::new();
        let mut by_set_pos = Vec::new();
        let mut wkst = None;

        loop {
            let name = r.spanned(Reader::ident)?;

            r.eat('=')?;

            if name.eq_ignore_ascii_case("FREQ") {
                freq = r.value().or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("UNTIL") {
                until = r.value().or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("COUNT") {
                count = r.value().or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("INTERVAL") {
                interval = r.value().or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYSECOND") {
                by_second = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYMINUTE") {
                by_minute = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYHOUR") {
                by_hour = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYDAY") {
                by_day = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYMONTHDAY") {
                by_month_day = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYYEARDAY") {
                by_year_day = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYWEEKNO") {
                by_week_no = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYMONTH") {
                by_month = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("BYSETPOS") {
                by_set_pos = r.value().unwrap_or_else(|| recover(r));
            } else if name.eq_ignore_ascii_case("WKST") {
                wkst = r.value().or_else(|| recover(r));
            } else {
                r.error(
                    name.span,
                    format!("unknown recurrence part: {}", name.value),
                );

                recover::<()>(r);
            }

            if r.try_eat(';').is_none() {
                break;
            }
        }

        let Some(freq) = freq else {
            r.error(Span::new(pos, pos + 1), "missing freq");
            return None;
        };

        Some(Self {
            freq,
            until,
            count,
            interval,
            by_second,
            by_minute,
            by_hour,
            by_day,
            by_month_day,
            by_year_day,
            by_week_no,
            by_month,
            by_set_pos,
            wkst,
        })
    }
}

impl Write<Value> for Recur {
    fn write(&self, w: &mut Writer) {
        w.raw("FREQ=");
        w.value(self.freq);

        w.param_opt("UNTIL", self.until.as_ref());
        w.param_opt("COUNT", self.count.as_ref());
        w.param_opt("INTERVAL", self.interval.as_ref());
        w.param_opt("WKST", self.wkst.as_ref());

        if !self.by_second.is_empty() {
            w.param("BYSECOND", &self.by_second);
        }
        if !self.by_minute.is_empty() {
            w.param("BYMINUTE", &self.by_minute);
        }
        if !self.by_hour.is_empty() {
            w.param("BYHOUR", &self.by_hour);
        }
        if !self.by_day.is_empty() {
            w.param("BYDAY", &self.by_day);
        }
        if !self.by_month_day.is_empty() {
            w.param("BYMONTHDAY", &self.by_month_day);
        }
        if !self.by_year_day.is_empty() {
            w.param("BYYEARDAY", &self.by_year_day);
        }
        if !self.by_week_no.is_empty() {
            w.param("BYWEEKNO", &self.by_week_no);
        }
        if !self.by_month.is_empty() {
            w.param("BYMONTH", &self.by_month);
        }
        if !self.by_set_pos.is_empty() {
            w.param("BYSETPOS", &self.by_set_pos);
        }
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

impl Read<Value> for Freq {
    fn read(r: &mut Reader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("SECONDLY") {
            Some(Freq::Secondly)
        } else if value.eq_ignore_ascii_case("MINUTELY") {
            Some(Freq::Minutely)
        } else if value.eq_ignore_ascii_case("HOURLY") {
            Some(Freq::Hourly)
        } else if value.eq_ignore_ascii_case("DAILY") {
            Some(Freq::Daily)
        } else if value.eq_ignore_ascii_case("WEEKLY") {
            Some(Freq::Weekly)
        } else if value.eq_ignore_ascii_case("MONTHLY") {
            Some(Freq::Monthly)
        } else if value.eq_ignore_ascii_case("YEARLY") {
            Some(Freq::Yearly)
        } else {
            r.error(span, format!("unknown freq `{value}`"));
            None
        }
    }
}

impl Write<Value> for Freq {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            Freq::Secondly => "SECONDLY",
            Freq::Minutely => "MINUTELY",
            Freq::Hourly => "HOURLY",
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
            Freq::Yearly => "YEARLY",
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByDay {
    /// E.g. `MO`
    Every(Weekday),

    /// E.g. `1TU`, `-2WE`
    Specific(NonZeroI8, Weekday),
}

impl Read<Value> for ByDay {
    fn read(r: &mut Reader) -> Option<Self> {
        match r.peek()? {
            '+' | '-' | '0'..='9' => {
                let sign = r.value::<Sign>()?;

                let nth = {
                    let Spanned { span, value } = r.spanned(u32::read)?;

                    let Ok(value) = i8::try_from(value) else {
                        r.error(span, "nth is too large");
                        return None;
                    };

                    let Some(value) = NonZeroI8::new(value) else {
                        r.error(span, "nth can't be zero");
                        return None;
                    };

                    match sign {
                        Sign::Neg => -value,
                        Sign::Pos => value,
                    }
                };

                Some(ByDay::Specific(nth, r.value()?))
            }

            _ => Some(ByDay::Every(r.value()?)),
        }
    }
}

impl Write<Value> for ByDay {
    fn write(&self, w: &mut Writer) {
        match self {
            ByDay::Every(day) => {
                w.value(day);
            }

            ByDay::Specific(nth, day) => {
                w.value(nth.get());
                w.value(day);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RecurViolation {
    #[error("interval is zero")]
    ZeroInterval,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ical, utils::*};
    use test_case::test_case;

    #[test]
    fn smoke() {
        let target = Recur::new(Freq::Minutely);

        let expected = ical! {"
            FREQ=MINUTELY
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_until() {
        let target = Recur::new(Freq::Minutely).with_until(dte("20180101T120000Z"));

        let expected = ical! {"
            FREQ=MINUTELY;UNTIL=20180101T120000Z
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_count() {
        let target = Recur::new(Freq::Minutely).with_count(123);

        let expected = ical! {"
            FREQ=MINUTELY;COUNT=123
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_interval() {
        let target = Recur::new(Freq::Minutely).with_interval(123);

        let expected = ical! {"
            FREQ=MINUTELY;INTERVAL=123
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_second() {
        let target = Recur::new(Freq::Minutely).with_by_second([
            Second::new(10).unwrap(),
            Second::new(20).unwrap(),
            Second::new(30).unwrap(),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYSECOND=10,20,30
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_minute() {
        let target = Recur::new(Freq::Minutely).with_by_minute([
            Minute::new(10).unwrap(),
            Minute::new(20).unwrap(),
            Minute::new(30).unwrap(),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYMINUTE=10,20,30
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_hour() {
        let target = Recur::new(Freq::Minutely).with_by_hour([
            Hour::new(4).unwrap(),
            Hour::new(8).unwrap(),
            Hour::new(12).unwrap(),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYHOUR=4,8,12
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_day() {
        let target = Recur::new(Freq::Minutely).with_by_day([
            ByDay::Every(Weekday::Monday),
            ByDay::Specific(NonZeroI8::new(1).unwrap(), Weekday::Tuesday),
            ByDay::Specific(NonZeroI8::new(-2).unwrap(), Weekday::Wednesday),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYDAY=MO,1TU,-2WE
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_month_day() {
        let target = Recur::new(Freq::Minutely).with_by_month_day([
            Signed::neg(Day::new(10).unwrap()),
            Signed::pos(Day::new(1).unwrap()),
            Signed::pos(Day::new(15).unwrap()),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYMONTHDAY=-10,1,15
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_year_day() {
        let target = Recur::new(Freq::Minutely).with_by_year_day([
            Signed::neg(DayOrdinal::new(69).unwrap()),
            Signed::pos(DayOrdinal::new(1).unwrap()),
            Signed::pos(DayOrdinal::new(120).unwrap()),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYYEARDAY=-69,1,120
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_week_no() {
        let target = Recur::new(Freq::Minutely).with_by_week_no([
            Signed::neg(WeekOrdinal::new(16).unwrap()),
            Signed::pos(WeekOrdinal::new(1).unwrap()),
            Signed::pos(WeekOrdinal::new(32).unwrap()),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYWEEKNO=-16,1,32
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_month() {
        let target = Recur::new(Freq::Minutely).with_by_month([
            Month::new(1).unwrap(),
            Month::new(6).unwrap(),
            Month::new(12).unwrap(),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYMONTH=1,6,12
        "};

        assert_eq!(expected, target.to_string(Value));
        assert_trip!(expected, Recur as Value);
    }

    #[test]
    fn with_by_set_pos() {
        let target = Recur::new(Freq::Minutely).with_by_set_pos([
            Signed::pos(DayOrdinal::new(1).unwrap()),
            Signed::pos(DayOrdinal::new(128).unwrap()),
            Signed::pos(DayOrdinal::new(366).unwrap()),
        ]);

        let expected = ical! {"
            FREQ=MINUTELY;BYSETPOS=1,128,366
        "};

        assert_eq!(expected, target.to_string(Value));
    }

    #[test]
    fn with_wkst() {
        let target = Recur::new(Freq::Minutely).with_wkst(Weekday::Monday);

        let expected = ical! {"
            FREQ=MINUTELY;WKST=MO
        "};

        assert_eq!(expected, target.to_string(Value));
    }

    #[test_case(Freq::Secondly, "SECONDLY")]
    #[test_case(Freq::Minutely, "MINUTELY")]
    #[test_case(Freq::Hourly, "HOURLY")]
    #[test_case(Freq::Daily, "DAILY")]
    #[test_case(Freq::Weekly, "WEEKLY")]
    #[test_case(Freq::Monthly, "MONTHLY")]
    #[test_case(Freq::Yearly, "YEARLY")]
    fn freq(obj: Freq, str: &str) {
        assert_eq!(str, obj.to_string(Value));
        assert_trip!(str, Freq as Value);
    }

    #[test]
    fn viol_zero_interval() {
        assert!(Recur::new(Freq::Monthly).validate().is_empty());

        assert!(
            Recur::new(Freq::Monthly)
                .with_interval(1)
                .validate()
                .is_empty()
        );

        assert_eq!(
            vec![RecurViolation::ZeroInterval],
            Recur::new(Freq::Monthly).with_interval(0).validate(),
        );
    }
}
