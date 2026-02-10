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
    /// Creates an RFC5545-compatible date.
    ///
    /// # Requirements
    ///
    /// - given date must exist, e.g. 31th of February will get rejected.
    pub fn new(year: Year, month: Month, day: Day) -> Result<Self, DateTimeViolation> {
        #[allow(clippy::cast_possible_wrap)]
        if JiffDate::new(
            year.as_num() as i16,
            month.as_num() as i8,
            day.as_num() as i8,
        )
        .is_err()
        {
            return Err(DateTimeViolation::UnknownDay {
                year: u32::from(year.as_num()),
                month: u32::from(month.as_num()),
                day: u32::from(day.as_num()),
            });
        }

        Ok(Self { year, month, day })
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

impl From<JiffDate> for Date {
    fn from(value: JiffDate) -> Self {
        #[allow(clippy::cast_sign_loss)]
        Self::new_unchecked(value.year() as u16, value.month() as u8, value.day() as u8)
    }
}

impl From<Date> for JiffDate {
    fn from(value: Date) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        jiff::civil::date(
            value.year.as_num() as i16,
            value.month.as_num() as i8,
            value.day.as_num() as i8,
        )
    }
}

impl TryFrom<Date> for JiffZoned {
    type Error = DateTimeError;

    fn try_from(value: Date) -> Result<Self, Self::Error> {
        Ok(JiffDate::from(value).at(0, 0, 0, 0).in_tz("UTC")?)
    }
}

impl IcsRead<Value> for Date {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.spanned(|r| {
            let y = r.spanned(|r| r.digits(4))?;
            let m = r.spanned(|r| r.digits(2))?;
            let d = r.spanned(|r| r.digits(2))?;

            Some(Self::new(
                y.map(Year::new).unwrap(r)?,
                m.map(Month::new).unwrap(r)?,
                d.map(Day::new).unwrap(r)?,
            ))
        })?
        .unwrap(r)
    }
}

impl IcsWrite<Value> for Date {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!(
            "{:04}{:02}{:02}",
            self.year.as_num(),
            self.month.as_num(),
            self.day.as_num()
        ));
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Date {
        const TYPE: PhpDataType = PhpDataType::Object(None);

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            Some(DateTime::<AnyForm>::from_zval(zval)?.date)
        }
    }

    impl IntoPhpZval for Date {
        const TYPE: PhpDataType = PhpDataType::Object(None);
        const NULLABLE: bool = false;

        fn set_zval(self, zv: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            DateTime {
                date: self,
                time: Time::new_unchecked(0, 0, 0),
                form: AnyForm::Utc,
            }
            .set_zval(zv, persistent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!("20180101", Date as Value);
        assert_trip!("19980101", Date as Value);
        assert_trip!("12341210", Date as Value);
    }

    #[test]
    fn viol_unknown_day() {
        let actual = Date::new(
            Year::new(2018).unwrap(),
            Month::new(2).unwrap(),
            Day::new(30).unwrap(),
        );

        let expected = Err(DateTimeViolation::UnknownDay {
            year: 2018,
            month: 2,
            day: 30,
        });

        assert_eq!(expected, actual);
    }
}
