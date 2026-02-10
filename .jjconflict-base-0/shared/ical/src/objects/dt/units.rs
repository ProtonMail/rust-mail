use super::*;

macro_rules! impls {
    ($ty:ident) => {
        impl IcsRead<Value> for $ty {
            fn read(r: &mut IcsReader) -> Option<Self> {
                r.value::<Spanned<u32>>()?.map(Self::new).unwrap(r)
            }
        }

        impl IcsWrite<Value> for $ty {
            fn write(&self, w: &mut IcsWriter) {
                w.value(self.0);
            }
        }

        #[cfg(feature = "php")]
        impl<'a> FromPhpZval<'a> for $ty {
            const TYPE: PhpDataType = PhpDataType::Long;

            fn from_zval(zval: &'a PhpZval) -> Option<Self> {
                let zval = zval.long()?;
                let zval = zval.try_into().ok()?;

                Self::new(zval).ok()
            }
        }

        #[cfg(feature = "php")]
        impl IntoPhpZval for $ty {
            const TYPE: PhpDataType = PhpDataType::Long;
            const NULLABLE: bool = false;

            fn set_zval(self, zval: &mut PhpZval, _: bool) -> PhpResult<()> {
                zval.set_long(self.0);

                Ok(())
            }
        }
    };
}

/// Year, aka `date-fullyear`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Year(u16);

impl Year {
    /// Creates an RFC5545-compatible year.
    ///
    /// # Requriements
    ///
    /// - given value must be between [0..9999].
    pub fn new(year: u32) -> Result<Self, DtUnitViolation> {
        if year > 9999 {
            return Err(DtUnitViolation::OutOfRangeYear(year));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(year as u16))
    }

    #[must_use]
    pub fn new_unchecked(year: u16) -> Self {
        Self(year)
    }

    #[must_use]
    pub fn as_num(&self) -> u16 {
        self.0
    }
}

impls!(Year);

/// Month, aka `date-month`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Month(u8);

impl Month {
    /// Creates an RFC5545-compatible month (1 = January).
    ///
    /// # Requriements
    ///
    /// - given value must be between [1..12].
    pub fn new(month: u32) -> Result<Self, DtUnitViolation> {
        if month == 0 || month > 12 {
            return Err(DtUnitViolation::OutOfRangeMonth(month));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(month as u8))
    }

    #[must_use]
    pub fn new_unchecked(month: u8) -> Self {
        Self(month)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(Month);

/// Day in the month, aka `date-mday`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Day(u8);

impl Day {
    /// Creates an RFC5545-compatible day.
    ///
    /// # Requriements
    ///
    /// - given value must be between [1..31].
    pub fn new(day: u32) -> Result<Self, DtUnitViolation> {
        if day == 0 || day > 31 {
            return Err(DtUnitViolation::OutOfRangeDay(day));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(day as u8))
    }

    #[must_use]
    pub fn new_unchecked(day: u8) -> Self {
        Self(day)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(Day);

/// Hour, aka `time-hour`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.12>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hour(u8);

impl Hour {
    /// Creates an RFC5545-compatible hour.
    ///
    /// # Requriements
    ///
    /// - given value must be between [0..23].
    pub fn new(hour: u32) -> Result<Self, DtUnitViolation> {
        if hour > 23 {
            return Err(DtUnitViolation::OutOfRangeHour(hour));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(hour as u8))
    }

    #[must_use]
    pub fn new_unchecked(hour: u8) -> Self {
        Self(hour)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(Hour);

/// Minute, aka `time-minute`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.12>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Minute(u8);

impl Minute {
    /// Creates an RFC5545-compatible second.
    ///
    /// # Requriements
    ///
    /// - given value must be between [0..59].
    pub fn new(minute: u32) -> Result<Self, DtUnitViolation> {
        if minute > 59 {
            return Err(DtUnitViolation::OutOfRangeMinute(minute));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(minute as u8))
    }

    #[must_use]
    pub fn new_unchecked(minute: u8) -> Self {
        Self(minute)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(Minute);

/// Second, aka `time-second`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.12>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Second(u8);

impl Second {
    /// Creates an RFC5545-compatible second.
    ///
    /// # Requriements
    ///
    /// - given value must be between [0..60].
    ///
    /// Note that the 60th second represents the leap second.
    pub fn new(second: u32) -> Result<Self, DtUnitViolation> {
        if second > 60 {
            return Err(DtUnitViolation::OutOfRangeSecond(second));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(second as u8))
    }

    #[must_use]
    pub fn new_unchecked(second: u8) -> Self {
        Self(second)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(Second);

/// Nth day of the year, aka `ordyrday`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.10>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayOrdinal(u16);

impl DayOrdinal {
    /// Creates an RFC5545-compatible year-day.
    ///
    /// # Requriements
    ///
    /// - given value must be between [1..366].
    pub fn new(day: u32) -> Result<Self, DtUnitViolation> {
        if day == 0 || day > 366 {
            return Err(DtUnitViolation::OutOfRangeDayOrdinal(day));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(day as u16))
    }

    #[must_use]
    pub fn new_unchecked(day: u16) -> Self {
        Self(day)
    }

    #[must_use]
    pub fn as_num(&self) -> u16 {
        self.0
    }
}

impls!(DayOrdinal);

/// Nth week of the year, aka `ordwk`.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.10>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeekOrdinal(u8);

impl WeekOrdinal {
    /// Creates an RFC5545-compatible year-day.
    ///
    /// # Requriements
    ///
    /// - given value must be between [1..53].
    pub fn new(week: u32) -> Result<Self, DtUnitViolation> {
        if week == 0 || week > 53 {
            return Err(DtUnitViolation::OutOfRangeWeekOrdinal(week));
        }

        #[allow(clippy::cast_possible_truncation)]
        Ok(Self::new_unchecked(week as u8))
    }

    #[must_use]
    pub fn new_unchecked(week: u8) -> Self {
        Self(week)
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.0
    }
}

impls!(WeekOrdinal);

/// Weekday, e.g. Monday or Tuesday.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.10>
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    #[must_use]
    pub fn as_jiff(self) -> JiffWeekday {
        match self {
            Weekday::Monday => JiffWeekday::Monday,
            Weekday::Tuesday => JiffWeekday::Tuesday,
            Weekday::Wednesday => JiffWeekday::Wednesday,
            Weekday::Thursday => JiffWeekday::Thursday,
            Weekday::Friday => JiffWeekday::Friday,
            Weekday::Saturday => JiffWeekday::Saturday,
            Weekday::Sunday => JiffWeekday::Sunday,
        }
    }
}

#[cfg(feature = "php")]
impl<'a> FromPhpZval<'a> for Weekday {
    const TYPE: PhpDataType = PhpDataType::String;

    fn from_zval(zval: &'a PhpZval) -> Option<Self> {
        match zval.str()? {
            "Monday" => Some(Weekday::Monday),
            "Tuesday" => Some(Weekday::Tuesday),
            "Wednesday" => Some(Weekday::Wednesday),
            "Thursday" => Some(Weekday::Thursday),
            "Friday" => Some(Weekday::Friday),
            "Saturday" => Some(Weekday::Saturday),
            "Sunday" => Some(Weekday::Sunday),
            _ => None,
        }
    }
}

#[cfg(feature = "php")]
impl IntoPhpZval for Weekday {
    const TYPE: PhpDataType = PhpDataType::String;
    const NULLABLE: bool = false;

    fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
        zval.set_string(&format!("{self:?}"), persistent)
    }
}

impl IcsRead<Value> for Weekday {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let Spanned { span, value } = r.spanned(IcsReader::ident)?;

        if value.eq_ignore_ascii_case("MO") {
            Some(Weekday::Monday)
        } else if value.eq_ignore_ascii_case("TU") {
            Some(Weekday::Tuesday)
        } else if value.eq_ignore_ascii_case("WE") {
            Some(Weekday::Wednesday)
        } else if value.eq_ignore_ascii_case("TH") {
            Some(Weekday::Thursday)
        } else if value.eq_ignore_ascii_case("FR") {
            Some(Weekday::Friday)
        } else if value.eq_ignore_ascii_case("SA") {
            Some(Weekday::Saturday)
        } else if value.eq_ignore_ascii_case("SU") {
            Some(Weekday::Sunday)
        } else {
            r.error(span, format!("unknown weekday `{value}`"));
            None
        }
    }
}

impl IcsWrite<Value> for Weekday {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Weekday::Monday => "MO",
            Weekday::Tuesday => "TU",
            Weekday::Wednesday => "WE",
            Weekday::Thursday => "TH",
            Weekday::Friday => "FR",
            Weekday::Saturday => "SA",
            Weekday::Sunday => "SU",
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DtUnitViolation {
    #[error("year `{0}` is out of range")]
    OutOfRangeYear(u32),

    #[error("year `{0}` is negative")]
    NegativeYear(i32),

    #[error("month `{0}` is out of range")]
    OutOfRangeMonth(u32),

    #[error("day `{0}` is out of range")]
    OutOfRangeDay(u32),

    #[error("hour `{0}` is out of range")]
    OutOfRangeHour(u32),

    #[error("minute `{0}` is out of range")]
    OutOfRangeMinute(u32),

    #[error("second `{0}` is out of range")]
    OutOfRangeSecond(u32),

    #[error("day ordinal (day of the year) `{0}` is out of range")]
    OutOfRangeDayOrdinal(u32),

    #[error("week ordinal (week of the year) `{0}` is out of range")]
    OutOfRangeWeekOrdinal(u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn year() {
        assert_eq!(Ok(Year(0)), Year::new(0));
        assert_eq!(Ok(Year(1)), Year::new(1));
        assert_eq!(Ok(Year(9999)), Year::new(9999));

        assert_eq!(
            Err(DtUnitViolation::OutOfRangeYear(10000)),
            Year::new(10000)
        );
    }

    #[test]
    fn month() {
        assert_eq!(Err(DtUnitViolation::OutOfRangeMonth(0)), Month::new(0));

        for m in 1..=12 {
            assert_eq!(m, u32::from(Month::new(m).unwrap().as_num()));
        }

        assert_eq!(Err(DtUnitViolation::OutOfRangeMonth(13)), Month::new(13));
    }

    #[test]
    fn day() {
        assert_eq!(Err(DtUnitViolation::OutOfRangeDay(0)), Day::new(0));

        for d in 1..=31 {
            assert_eq!(d, u32::from(Day::new(d).unwrap().as_num()));
        }

        assert_eq!(Err(DtUnitViolation::OutOfRangeDay(32)), Day::new(32));
    }

    #[test]
    fn hour() {
        for h in 0..=23 {
            assert_eq!(h, u32::from(Hour::new(h).unwrap().as_num()));
        }

        assert_eq!(Err(DtUnitViolation::OutOfRangeHour(24)), Hour::new(24));
    }

    #[test]
    fn minute() {
        for h in 0..=59 {
            assert_eq!(h, u32::from(Minute::new(h).unwrap().as_num()));
        }

        assert_eq!(Err(DtUnitViolation::OutOfRangeMinute(60)), Minute::new(60));
    }

    #[test]
    fn second() {
        for s in 0..=60 {
            assert_eq!(s, u32::from(Second::new(s).unwrap().as_num()));
        }

        assert_eq!(Err(DtUnitViolation::OutOfRangeSecond(61)), Second::new(61));
    }

    #[test]
    fn day_ordinal() {
        assert_eq!(
            Err(DtUnitViolation::OutOfRangeDayOrdinal(0)),
            DayOrdinal::new(0)
        );

        for d in 1..=366 {
            assert_eq!(d, u32::from(DayOrdinal::new(d).unwrap().as_num()));
        }

        assert_eq!(
            Err(DtUnitViolation::OutOfRangeDayOrdinal(367)),
            DayOrdinal::new(367)
        );
    }

    #[test]
    fn week_ordinal() {
        assert_eq!(
            Err(DtUnitViolation::OutOfRangeWeekOrdinal(0)),
            WeekOrdinal::new(0)
        );

        for d in 1..=53 {
            assert_eq!(d, u32::from(WeekOrdinal::new(d).unwrap().as_num()));
        }

        assert_eq!(
            Err(DtUnitViolation::OutOfRangeWeekOrdinal(54)),
            WeekOrdinal::new(54)
        );
    }

    #[test_case(Weekday::Monday, "MO")]
    #[test_case(Weekday::Tuesday, "TU")]
    #[test_case(Weekday::Wednesday, "WE")]
    #[test_case(Weekday::Thursday, "TH")]
    #[test_case(Weekday::Friday, "FR")]
    #[test_case(Weekday::Saturday, "SA")]
    #[test_case(Weekday::Sunday, "SU")]
    fn weekday(obj: Weekday, str: &str) {
        assert_eq!(str, obj.to_string(Value));
        assert_trip!(str, Weekday as Value);
    }
}
