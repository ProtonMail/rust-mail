use super::*;

/// UTC offset; [-23:59:59 .. +23:59:59].
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.3.14>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcOffset {
    offset: i32,
}

impl UtcOffset {
    /// Creates an RFC5545-compatible offset.
    ///
    /// # Requirements
    ///
    /// Offset must be between [-23:59:59 .. +23:59:59].
    pub fn new(
        sign: Sign,
        hours: u32,
        minutes: u32,
        seconds: u32,
    ) -> Result<Self, UtcOffsetViolation> {
        if hours > 23 {
            return Err(UtcOffsetViolation::OutOfRangeHourOffset(hours));
        }

        if minutes > 59 {
            return Err(UtcOffsetViolation::OutOfRangeMinuteOffset(minutes));
        }

        if seconds > 59 {
            return Err(UtcOffsetViolation::OutOfRangeSecondOffset(seconds));
        }

        let offset = (hours * 60 * 60) + (minutes * 60) + seconds;

        #[allow(clippy::cast_possible_wrap)]
        let offset = match sign {
            Sign::Pos => offset as i32,
            Sign::Neg => -(offset as i32),
        };

        Ok(Self { offset })
    }

    #[must_use]
    pub fn new_unchecked(offset: i32) -> Self {
        Self { offset }
    }

    #[must_use]
    pub fn sign(&self) -> Sign {
        if self.offset < 0 {
            Sign::Neg
        } else {
            Sign::Pos
        }
    }

    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset.unsigned_abs()
    }

    #[must_use]
    pub fn hours(&self) -> u32 {
        self.offset() / (60 * 60)
    }

    #[must_use]
    pub fn minutes(&self) -> u32 {
        (self.offset() / 60) % 60
    }

    #[must_use]
    pub fn seconds(&self) -> u32 {
        self.offset() % 60
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum UtcOffsetViolation {
    #[error("hour offset `{0}` is out of range")]
    OutOfRangeHourOffset(u32),

    #[error("minute offset `{0}` is out of range")]
    OutOfRangeMinuteOffset(u32),

    #[error("second offset `{0}` is out of range")]
    OutOfRangeSecondOffset(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new() {
        for sign in [Sign::Neg, Sign::Pos] {
            for hours in 0..24 {
                for minutes in 0..60 {
                    for seconds in 0..60 {
                        let target = UtcOffset::new(sign, hours, minutes, seconds).unwrap();

                        if hours == 0 && minutes == 0 && seconds == 0 {
                            // RFC says that the `-0000` offset is illegal, so
                            // let's make sure that the zero offset always has
                            // the positive sign
                            assert_eq!(Sign::Pos, target.sign());
                        } else {
                            assert_eq!(sign, target.sign());
                        }

                        assert_eq!(hours, target.hours());
                        assert_eq!(minutes, target.minutes());
                        assert_eq!(seconds, target.seconds());
                    }
                }
            }
        }

        assert_eq!(
            Err(UtcOffsetViolation::OutOfRangeHourOffset(24)),
            UtcOffset::new(Sign::Pos, 24, 0, 0)
        );
        assert_eq!(
            Err(UtcOffsetViolation::OutOfRangeMinuteOffset(60)),
            UtcOffset::new(Sign::Pos, 0, 60, 0)
        );
        assert_eq!(
            Err(UtcOffsetViolation::OutOfRangeSecondOffset(60)),
            UtcOffset::new(Sign::Pos, 0, 0, 60)
        );
    }
}
