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

impl From<JiffTime> for Time {
    fn from(value: JiffTime) -> Self {
        #[allow(clippy::cast_sign_loss)]
        Self::new_unchecked(
            value.hour() as u8,
            value.minute() as u8,
            value.second() as u8,
        )
    }
}

impl From<Time> for JiffTime {
    fn from(value: Time) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        jiff::civil::time(
            value.hour.as_num() as i8,
            value.minute.as_num() as i8,
            value.second.as_num() as i8,
            0,
        )
    }
}

impl IcsRead<Value> for Time {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let hour = r.spanned(|r| r.digits(2))?;
        let minute = r.spanned(|r| r.digits(2))?;
        let second = r.spanned(|r| r.digits(2))?;

        let hour = hour.map(Hour::new).unwrap(r)?;
        let minute = minute.map(Minute::new).unwrap(r)?;
        let second = second.map(Second::new).unwrap(r)?;

        Some(Self {
            hour,
            minute,
            second,
        })
    }
}

impl IcsWrite<Value> for Time {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!(
            "{:02}{:02}{:02}",
            self.hour.as_num(),
            self.minute.as_num(),
            self.second.as_num()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!("000000", Time as Value);
        assert_trip!("000030", Time as Value);
        assert_trip!("000059", Time as Value);
        assert_trip!("000060", Time as Value);
        assert_trip!("010203", Time as Value);
        assert_trip!("120000", Time as Value);
        assert_trip!("235959", Time as Value);
    }
}
