//! This module implements the recurrence rule iterator - it allows to retrieve
//! all occurrences of an event (i.e. the dates when it repeats) and supports a
//! couple of quality-of-life features such as fast-forwarding.
//!
//! See [`VEvent::occurrences()`] for example usage.
//!
//! # Implementation
//!
//! Let's start with the problem - given a recurrence rule:
//!
//! ```text
//! FREQ=DAILY;BYDAY=FR;BYMONTHDAY=13
//! -- every Friday 13th
//!
//! FREQ=MONTHLY;BYMONTHDAY=10,20,30
//! -- every month at its 10th, 20th and 30th day
//!
//! FREQ=YEAR;BYDAY=3MO,5MO
//! -- every year's third Monday and fifth Monday
//! ```
//!
//! ... how do we figure out when it repeats?
//!
//! Isolated, the rules are quite obvious - e.g. MONTHLY + BYMONTHDAY is
//! essentially:
//!
//! ```text
//! fn iter_monthly(
//!     recur: &Recur,
//!     start: DateTime,
//! ) -> impl Iterator<Item = DateTime> {
//!     for year in start.year().. {
//!         for md in recur.by_month_day {
//!             yield start.with_year(year).with_month_day(md);
//!         }
//!     }
//! }
//! ```
//!
//! ... but as soon as you make a step forward, you stumble upon an annoying
//! problem - depending on context, a rule functions either as a filter (`if`):
//!
//! ```text
//! FREQ=DAILY;BYDAY=MO,TU
//!
//! for day in start.series() {
//!     if day.weekday() == Weekday::Monday || day.weekday() == Weekday::Tuesday {
//!         yield day;
//!     }
//! }
//! ```
//!
//! ... or as a generator (`for`):
//!
//! ```text
//! FREQ=MONTHLY;BYDAY=MO,TU
//!
//! for (year, month) in start.series() {
//!     for day in [Weekday::Monday, Weekday::Tuesday] {
//!         yield Date::new(year, month, day);
//!     }
//! }
//! ```
//!
//! RFC - <https://www.rfc-editor.org/rfc/rfc5545> - collects all of those cases
//! into a rather spooky-looking table at the bottom of page #44, which miiight
//! imply having a FREQ-dependent logic:
//!
//! ```text
//! fn iter(recur: &Recur, start: DateTime) -> impl Iterator<Item = DateTime> {
//!     match recur.freq {
//!         Freq::Monthly => iter_monthly(recur, start),
//!         Freq::Daily => iter_daily(recur, start),
//!         /* ... */
//!     }
//! }
//!
//! fn iter_monthly(...) -> ... {
//!     // BYDAY is generator, BYMONTHDAY is generator, ...
//! }
//!
//! fn iter_daily(...) -> ... {
//!     // BYDAY is filter, BYMONTHDAY is filter, ...
//! }
//! ```
//!
//! ... which is a pity, because it requires you to reimplement a lot of stuff,
//! which usually end up in libraries not supporting arbitrary combinations of
//! features that people forgot to implement or thought are unneeded, like with
//! <https://github.com/libical/libical/issues/795>.
//!
//! Instead of having this FREQ-specific logic, we can borrow a cool thing from
//! the rendering community - signed distance functions!
//!
//! <https://www.shadertoy.com/view/4slSWf>
//!
//! Long story short - we define a couple of primitives, a couple of functions
//! that take a date, return a date-span, and satisfy the following properties:
//!
//! - if given date matches the rule, the function returns an empty span (i.e.
//!   0 seconds),
//!
//! - if given date doesn't match the rule, the function returns either an exact
//!   next occurrence or an *underapproximation* of one.
//!
//! For instance, considering that 2018-01-01 is Monday:
//!
//! ```text
//! (weekday-eq Monday 2018-01-01) = 0s
//! (weekday-eq Tuesday 2018-01-01) = +1d
//! (weekday-eq Friday 2018-01-01) = +5d  -- any value from +1d up to +5d would be legal
//!
//! (weekday-eq Tuesday 2018-01-02) = 0s
//! (weekday-eq Monday 2018-01-02) = +6d  -- any value from +1d up to +6d would be legal
//! ```
//!
//! We define a couple of those primitives, such as `weekday-eq` (corresponding
//! to the `BYDAY` rule) or `month-eq` (corresponding to the `BYMONTH` rule),
//! and two boolean operators - `or` + `and`, where:
//!
//! ```text
//! (or A B C) = (min A B C)
//!
//! e.g.:
//!
//! (or (weekday-eq Monday) (weekday-eq Wednesday) (weekday-eq Friday) 2018-01-02)
//! = (or +6d +1d +3d)
//! = +1d
//! ```
//!
//! ... which reduces our problem to just converting rules into constraints:
//!
//! ```text
//! FREQ=MONTHLY;BYMONTHDAY=10,20,30;BYDAY=MO
//!
//! (and
//!   (or (day-eq 10) (day-eq 20) (day-eq 30))
//!   (weekday-eq Monday))
//! ```
//!
//! ... and then iterating through them:
//!
//! ```text
//! fn iter(recur: &Recur, start: DateTime) -> impl Iterator<Item = DateTime> {
//!     let fact = recur.as_fact();
//!     let mut date = start;
//!
//!     loop {
//!         let span = fact.next_occur_of(start);
//!
//!         if span.is_zero() {
//!             yield date;
//!
//!             // Add an epsilon value to get the ball going on in the next
//!             // iteration:
//!             date += Span::second(1);
//!         } else {
//!             date += span;
//!         }
//!     }
//! }
//! ```
//!
//! This is a nice approach, for a couple of reasons:
//!
//! - all of those primitives are easily testable in isolation,
//!
//! - instead of having FREQ * BY-params code paths, we end up with FREQ +
//!   BY-params paths,
//!
//! - since spans are either zero (in which case we add an epsilon) or positive,
//!   we have a guarantee that the main `loop` always moves forward, it doesn't
//!   get stuck.

mod fact;
mod picker;

use self::fact::*;
use self::picker::*;
use super::*;

/// Iterator over occurrences of an event, see [`VEvent::occurrences()`].
#[derive(Clone)]
pub struct RecurIterator {
    /// Repetition's beginning, aka `DTSTART`.
    start: JiffZoned,

    /// Repetition's frequency, e.g. daily or monthly.
    freq: Freq,

    /// Repetition's interval, e.g. every day (1) or every three months (3).
    interval: i64,

    /// Repetition's constraints, e.g. `BYDAY=MO`.
    fact: Fact,

    /// Picker, used to group dates that match the `BYSETPOS` rule.
    picker: Option<Picker<JiffZoned>>,

    /// Next repetition, but taking into account *only* `start`, `freq`, and
    /// `interval`.
    ///
    /// This is used to implement the `BYSETPOS` rule - it's `None` when
    /// `BYSETPOS` was not specified.
    repeat: Option<JiffZoned>,

    /// Currently iterated-over date.
    curr: JiffZoned,

    /// When present, we'll emit this specific date as the next one; boxed to
    /// reduce the iterator's size.
    next: Option<Box<JiffZoned>>,

    /// When present, we'll skip all dates until this one; boxed to reduce the
    /// iterator's size.
    ///
    /// This is used to implement fast-forwarding.
    since: Option<Box<JiffZoned>>,

    /// When present, we'll stop after reaching this date; boxed to reduce the
    /// iterator size.
    until: Option<Box<JiffZoned>>,

    /// When present, we'll stop after emitting this number of items.
    count: Option<u32>,

    /// Numer of emitted items.
    emitted: u32,
}

impl RecurIterator {
    pub fn new(recur: &Recur, start: impl Into<DateOrDt>) -> Result<Self, RecurIteratorError> {
        if recur.interval == Some(0) {
            return Err(RecurIteratorError::ZeroInterval);
        }

        let start =
            JiffZoned::try_from(start.into()).map_err(RecurIteratorError::InvalidDtStart)?;

        let freq = recur.freq;
        let interval = i64::from(recur.interval.unwrap_or(1));
        let fact = Fact::from_recur(recur, &start).map_err(RecurIteratorError::InvalidRecur)?;

        let repeat = if recur.by_set_pos.is_empty() {
            // This field only exists to support implementing the BYSETPOS rule
            // - if there's no BYSETPOS, no point in having this field
            None
        } else {
            Some(
                Fact::next_instance_of(&start, freq, interval, &start)
                    .unwrap_or_else(|_| start.clone()),
            )
        };

        let picker = Picker::new(recur.by_set_pos.iter().filter_map(|idx| {
            let val = i16::try_from(idx.value.as_num()).ok()?;

            if idx.sign.is_neg() {
                Some(-val)
            } else {
                Some(val)
            }
        }));

        let curr = if recur.by_set_pos.is_empty() {
            start.clone()
        } else {
            // `BYSETPOS` counts sub-occurrences from the beginning of the
            // frequency, so if this rule is present, we must start our search
            // earlier in order to get the indices correct.
            //
            // E.g. given `FREQ=MONTHLY;BYDAY=MO;BYSETPOS=2;DTSTART=20180102`,
            // we must count 2018-01-01 as the first sub-occurrence so that we
            // emit 2018-01-08, not 2018-01-15.
            freq.first_of(&start)
                .map_err(DateTimeError::Jiff)
                .map_err(RecurIteratorError::InvalidDtStart)?
        };

        #[allow(clippy::redundant_closure_for_method_calls)]
        let until = recur
            .until
            .map(JiffZoned::try_from)
            .transpose()
            .map_err(RecurIteratorError::InvalidUntil)?
            .map(Box::new);

        Ok(Self {
            start,
            freq,
            interval,
            fact,
            picker,
            repeat,
            curr,
            next: None,
            since: None,
            until,
            count: recur.count,
            emitted: 0,
        })
    }

    /// Fast-forwards the iterator; see [`Self::set_since()`].
    #[must_use]
    pub fn since(mut self, at: JiffZoned) -> Self {
        self.set_since(at);
        self
    }

    /// Fast-fowards the iterator.
    ///
    /// Having called this, the next invocation of [`Self::next()`] will return
    /// either `Some(dt)` where `dt >= at` or `None` (e.g. if `rrule.until` got
    /// reached).
    ///
    /// This function has constant time complexity, it doesn't matter how far
    /// you want to jump (by one day, by one month, by one year, ...).
    ///
    /// Calling this function disables the `rrule.count` limit, since we can't
    /// reasonably count how many dates are being skipped over.
    pub fn set_since(&mut self, at: JiffZoned) {
        if let (Some(picker), Some(repeat)) = (&mut self.picker, &mut self.repeat) {
            picker.reset();

            *repeat = Fact::next_instance_of(&self.start, self.freq, self.interval, &at)
                .unwrap_or_else(|_| at.clone());

            self.curr = self.freq.first_of(&at).unwrap_or_else(|_| at.clone());
            self.since = Some(Box::new(at));
        } else {
            self.curr = at;
            self.since = None;
        }

        self.next = None;
        self.count = None;

        // Pretend we've already emitted one event so that we don't emit
        // DTSTART as it doesn't make sense after fast-forwarding
        self.emitted = 1;
    }

    fn next_inner(&mut self) -> Option<JiffZoned> {
        if let Some(next) = self.next.take() {
            return Some(*next);
        }

        if let Some(picker) = &mut self.picker {
            if let Some(next) = picker.pull() {
                return Some(next);
            }
        }

        loop {
            let next = self.fact.next_occur_of(&self.curr).ok()?;

            if let (Some(picker), Some(repeat)) = (&mut self.picker, &mut self.repeat) {
                if next >= *repeat {
                    *repeat = Fact::next_instance_of(&self.start, self.freq, self.interval, &next)
                        .ok()?;

                    picker.close();

                    if let Some(next) = picker.pull() {
                        return Some(next);
                    }
                }
            }

            #[cfg(test)]
            if next.year() >= 2100 {
                // Short-circuit so that if the code misbehaves, tests can have
                // an early-exit
                return None;
            }

            match next.cmp(&self.curr) {
                Ordering::Less => {
                    #[cfg(debug_assertions)]
                    panic!("went back in time");

                    #[cfg(not(debug_assertions))]
                    return None;
                }

                Ordering::Equal => {
                    self.curr = self.curr.checked_add(JiffSpan::new().seconds(1)).ok()?;

                    if let Some(picker) = &mut self.picker {
                        if let Some(next) = picker.push(next) {
                            return Some(next);
                        }
                    } else {
                        return Some(next);
                    }
                }

                Ordering::Greater => {
                    self.curr = next;
                }
            }
        }
    }
}

impl Iterator for RecurIterator {
    type Item = JiffZoned;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(count) = self.count {
            if self.emitted >= count {
                return None;
            }
        }

        let curr = loop {
            let curr = self.next_inner()?;

            // Skip dates that have happened before `DTSTART`.
            //
            // E.g. given `FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1;DTSTART=20180109`,
            // the first date we'll witness is 2018-01-01, since that's the
            // first Monday of 2018-01 - but we can't actually emit it, because
            // it's happened before 2018-01-09.
            if curr < self.start {
                continue;
            }

            // Skip dates that have happened before the minimum date.
            //
            // This is basically the same condition as above, but adjusted for
            // fast-forwarding.
            if let Some(since) = &self.since {
                if curr < **since {
                    continue;
                }
            }

            break curr;
        };

        if let Some(until) = &self.until {
            if curr > **until {
                return None;
            }
        }

        self.emitted += 1;

        // Make sure we emit `DTSTART` as the first item, even if it doesn't
        // match the recurrence rules.
        //
        // E.g. given `FREQ=MONTHLY;BYDAY=MO;DTSTART=20180102`, emit 2018-01-02
        // first, even though 2018-01-02 is actually a Tuesday.
        //
        // Various libraries seem to be on the fence in terms of this behavior,
        // but the RFC does say:
        //
        // > The "DTSTART" property value always counts as the first occurrence.
        //
        // ... which sorta implies that we should emit `DTSTART` regardless of
        // whether it actually matches the rules or not.
        //
        // This ambiguity got later addressed:
        //
        // > The "DTSTART" property SHOULD be synchronized with the recurrence
        // > rule, if specified.
        //
        // ... which means we could just make a mismatched `DTSTART` an error,
        // but since we can handle this case basically for free, we not do it.
        if self.emitted == 1 && curr != self.start {
            self.next = Some(Box::new(curr));

            return Some(self.start.clone());
        }

        Some(curr)
    }
}

#[derive(Debug, Error)]
pub enum RecurIteratorError {
    #[error("invalid rrule.interval: can't be zero")]
    ZeroInterval,

    #[error("missing dtstart")]
    MissingDtStart,

    #[error("invalid dtstart: {0}")]
    InvalidDtStart(DateTimeError),

    #[error("invalid rrule.until: {0}")]
    InvalidUntil(DateTimeError),

    #[error("invalid rrule: {0}")]
    InvalidRecur(JiffError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;
    use pretty_assertions as pa;

    fn target(recur_s: &str, start_s: &str) -> RecurIterator {
        let recur = recur(recur_s);

        let start = if start_s.contains('T') {
            DateOrDt::DateTime(dt(start_s))
        } else {
            DateOrDt::Date(d(start_s))
        };

        RecurIterator::new(&recur, start).unwrap()
    }

    #[test]
    fn secondly() {
        let actual: Vec<_> = target("FREQ=SECONDLY", "20180101T180000").take(3).collect();

        let expected = vec![
            jz("2018-01-01 18:00:00"),
            jz("2018-01-01 18:00:01"),
            jz("2018-01-01 18:00:02"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn secondly_with_interval() {
        let actual: Vec<_> = target("FREQ=SECONDLY;INTERVAL=3", "20180101T180000")
            .take(3)
            .collect();

        let expected = vec![
            jz("2018-01-01 18:00:00"),
            jz("2018-01-01 18:00:03"),
            jz("2018-01-01 18:00:06"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn secondly_with_by_second() {
        let actual: Vec<_> = target("FREQ=SECONDLY;BYSECOND=10,20,30", "20180101T180010")
            .take(6)
            .collect();

        let expected = vec![
            jz("2018-01-01 18:00:10"),
            jz("2018-01-01 18:00:20"),
            jz("2018-01-01 18:00:30"),
            jz("2018-01-01 18:01:10"),
            jz("2018-01-01 18:01:20"),
            jz("2018-01-01 18:01:30"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn minutely() {
        let actual: Vec<_> = target("FREQ=MINUTELY", "20180101T180000").take(3).collect();

        let expected = vec![
            jz("2018-01-01 18:00:00"),
            jz("2018-01-01 18:01:00"),
            jz("2018-01-01 18:02:00"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn minutely_with_interval() {
        let actual: Vec<_> = target("FREQ=MINUTELY;INTERVAL=3", "20180101T180000")
            .take(3)
            .collect();

        let expected = vec![
            jz("2018-01-01 18:00:00"),
            jz("2018-01-01 18:03:00"),
            jz("2018-01-01 18:06:00"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn minutely_with_by_minute() {
        let actual: Vec<_> = target("FREQ=MINUTELY;BYMINUTE=10,20,30", "20180101T181000")
            .take(6)
            .collect();

        let expected = vec![
            jz("2018-01-01 18:10:00"),
            jz("2018-01-01 18:20:00"),
            jz("2018-01-01 18:30:00"),
            jz("2018-01-01 19:10:00"),
            jz("2018-01-01 19:20:00"),
            jz("2018-01-01 19:30:00"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn hourly() {
        let actual: Vec<_> = target("FREQ=HOURLY", "20180101T183456").take(3).collect();

        let expected = vec![
            jz("2018-01-01 18:34:56"),
            jz("2018-01-01 19:34:56"),
            jz("2018-01-01 20:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn hourly_with_interval() {
        let actual: Vec<_> = target("FREQ=HOURLY;INTERVAL=3", "20180101T183456")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-01 18:34:56"),
            jz("2018-01-01 21:34:56"),
            jz("2018-01-02 00:34:56"),
            jz("2018-01-02 03:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn hourly_with_by_hour() {
        let actual: Vec<_> = target("FREQ=HOURLY;BYHOUR=10,12,14", "20180101T103456")
            .take(6)
            .collect();

        let expected = vec![
            jz("2018-01-01 10:34:56"),
            jz("2018-01-01 12:34:56"),
            jz("2018-01-01 14:34:56"),
            jz("2018-01-02 10:34:56"),
            jz("2018-01-02 12:34:56"),
            jz("2018-01-02 14:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn daily() {
        let actual: Vec<_> = target("FREQ=DAILY", "20180101").take(3).collect();
        let expected = vec![jz("2018-01-01"), jz("2018-01-02"), jz("2018-01-03")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=DAILY", "20180101T123456").take(3).collect();

        let expected = vec![
            jz("2018-01-01 12:34:56"),
            jz("2018-01-02 12:34:56"),
            jz("2018-01-03 12:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn daily_with_interval() {
        let actual: Vec<_> = target("FREQ=DAILY;INTERVAL=3", "20180101")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-01"),
            jz("2018-01-04"),
            jz("2018-01-07"),
            jz("2018-01-10"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn daily_with_by_day() {
        let actual: Vec<_> = target("FREQ=DAILY;BYDAY=TU,WE", "20180102")
            .take(6)
            .collect();

        let expected = vec![
            jz("2018-01-02"),
            jz("2018-01-03"),
            jz("2018-01-09"),
            jz("2018-01-10"),
            jz("2018-01-16"),
            jz("2018-01-17"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn weekly() {
        let actual: Vec<_> = target("FREQ=WEEKLY", "20180101").take(3).collect();
        let expected = vec![jz("2018-01-01"), jz("2018-01-08"), jz("2018-01-15")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=WEEKLY", "20180101T123456").take(3).collect();

        let expected = vec![
            jz("2018-01-01 12:34:56"),
            jz("2018-01-08 12:34:56"),
            jz("2018-01-15 12:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn weekly_with_interval() {
        let actual: Vec<_> = target("FREQ=WEEKLY;INTERVAL=3", "20180101")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-01"),
            jz("2018-01-22"),
            jz("2018-02-12"),
            jz("2018-03-05"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn weekly_with_by_day() {
        let actual: Vec<_> = target("FREQ=WEEKLY;BYDAY=TU,WE", "20180102")
            .take(6)
            .collect();

        let expected = vec![
            jz("2018-01-02"),
            jz("2018-01-03"),
            jz("2018-01-09"),
            jz("2018-01-10"),
            jz("2018-01-16"),
            jz("2018-01-17"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly() {
        let actual: Vec<_> = target("FREQ=MONTHLY", "20180107").take(3).collect();
        let expected = vec![jz("2018-01-07"), jz("2018-02-07"), jz("2018-03-07")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=MONTHLY", "20180107T123456").take(3).collect();

        let expected = vec![
            jz("2018-01-07 12:34:56"),
            jz("2018-02-07 12:34:56"),
            jz("2018-03-07 12:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly_with_interval() {
        let actual: Vec<_> = target("FREQ=MONTHLY;INTERVAL=3", "20180107")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-07"),
            jz("2018-04-07"),
            jz("2018-07-07"),
            jz("2018-10-07"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly_with_by_day() {
        let actual: Vec<_> = target("FREQ=MONTHLY;BYDAY=-2FR", "20180119")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-19"),
            jz("2018-02-16"),
            jz("2018-03-23"),
            jz("2018-04-20"),
            jz("2018-05-18"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly_with_by_month_day() {
        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=-28,2", "20180102")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-02"),
            jz("2018-01-04"),
            jz("2018-02-01"),
            jz("2018-02-02"),
            jz("2018-03-02"),
        ];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=MONTHLY;INTERVAL=5;BYMONTHDAY=1,31,-7", "20110101")
            .take(9)
            .collect();

        let expected = vec![
            jz("2011-01-01"),
            jz("2011-01-25"),
            jz("2011-01-31"),
            jz("2011-06-01"),
            jz("2011-06-24"),
            // 2011-06-31 doesn't exist
            jz("2011-11-01"),
            jz("2011-11-24"),
            // 2011-11-31 doesn't exist
            jz("2012-04-01"),
            jz("2012-04-24"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly_with_by_month_day_that_underflows() {
        // This is a gray area - RFC doesn't state what happens on underflow.

        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=-31", "20180101")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-01"),
            jz("2018-03-01"),
            jz("2018-05-01"),
            jz("2018-07-01"),
            jz("2018-08-01"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn monthly_with_by_set_pos() {
        let actual: Vec<_> = target("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=3", "20180115")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-15"),
            jz("2018-02-19"),
            jz("2018-03-19"),
            jz("2018-04-16"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly() {
        let actual: Vec<_> = target("FREQ=YEARLY", "20180307").take(3).collect();
        let expected = vec![jz("2018-03-07"), jz("2019-03-07"), jz("2020-03-07")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=YEARLY", "20180307T123456").take(3).collect();

        let expected = vec![
            jz("2018-03-07 12:34:56"),
            jz("2019-03-07 12:34:56"),
            jz("2020-03-07 12:34:56"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_interval() {
        let actual: Vec<_> = target("FREQ=YEARLY;INTERVAL=3", "20180307")
            .take(3)
            .collect();

        let expected = vec![jz("2018-03-07"), jz("2021-03-07"), jz("2024-03-07")];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_by_month() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYMONTH=4,7", "20180401")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-04-01"),
            jz("2018-07-01"),
            jz("2019-04-01"),
            jz("2019-07-01"),
            jz("2020-04-01"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_by_month_and_by_month_day() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYMONTH=4,7;BYMONTHDAY=5,8", "20180405")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-04-05"),
            jz("2018-04-08"),
            jz("2018-07-05"),
            jz("2018-07-08"),
            jz("2019-04-05"),
        ];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=YEARLY;BYMONTH=1,2,3,4,5,6;BYMONTHDAY=31", "20180131")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-31"),
            jz("2018-03-31"),
            jz("2018-05-31"),
            jz("2019-01-31"),
            jz("2019-03-31"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_by_year_day() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYYEARDAY=-1,1", "20180101")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-01"),
            jz("2018-12-31"),
            jz("2019-01-01"),
            jz("2019-12-31"),
            jz("2020-01-01"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_by_year_day_that_underflows() {
        // This is a gray area - RFC doesn't state what happens on underflow.

        let actual: Vec<_> = target("FREQ=YEARLY;BYYEARDAY=-366", "20180101")
            .take(5)
            .collect();

        let expected = vec![
            jz("2018-01-01"),
            jz("2018-12-31"),
            jz("2019-12-31"),
            jz("2020-01-01"),
            jz("2022-12-31"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn yearly_with_by_set_pos() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYYEARDAY=1,30,60;BYSETPOS=2,3", "20180130")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-01-30"),
            jz("2018-03-01"),
            jz("2019-01-30"),
            jz("2019-03-01"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn until() {
        let actual: Vec<_> = target("FREQ=DAILY;UNTIL=20170101", "20180101").collect();
        let expected = Vec::<JiffZoned>::new();

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=DAILY;UNTIL=20180101", "20180101").collect();
        let expected = vec![jz("2018-01-01")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=DAILY;UNTIL=20180103", "20180101").collect();
        let expected = vec![jz("2018-01-01"), jz("2018-01-02"), jz("2018-01-03")];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn count() {
        let actual: Vec<_> = target("FREQ=DAILY;COUNT=1", "20180101").collect();
        let expected = vec![jz("2018-01-01")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=DAILY;COUNT=2", "20180101").collect();
        let expected = vec![jz("2018-01-01"), jz("2018-01-02")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=DAILY;COUNT=3", "20180101").collect();
        let expected = vec![jz("2018-01-01"), jz("2018-01-02"), jz("2018-01-03")];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target(
            "FREQ=MONTHLY;BYDAY=-1MO,-1TU,-1WE;BYSETPOS=-1;COUNT=5",
            "20180101",
        )
        .collect();

        let expected = vec![
            jz("2018-01-01"), // via DTSTART
            jz("2018-01-31"), // via BYDAY=-1WE
            jz("2018-02-28"), // via BYDAY=-1WE
            jz("2018-03-28"), // via BYDAY=-1WE
            jz("2018-04-30"), // via BYDAY=-1MO
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn lots_of_rules() {
        let actual: Vec<_> = target(
            "FREQ=MONTHLY;INTERVAL=2;BYDAY=1SU,4SU;BYHOUR=15,17;BYMINUTE=30,32;BYSECOND=11,12",
            "20010101T123456",
        )
        .take(33)
        .collect();

        let expected = vec![
            jz("2001-01-01 12:34:56"),
            jz("2001-01-07 15:30:11"),
            jz("2001-01-07 15:30:12"),
            jz("2001-01-07 15:32:11"),
            jz("2001-01-07 15:32:12"),
            jz("2001-01-07 17:30:11"),
            jz("2001-01-07 17:30:12"),
            jz("2001-01-07 17:32:11"),
            jz("2001-01-07 17:32:12"),
            jz("2001-01-28 15:30:11"),
            jz("2001-01-28 15:30:12"),
            jz("2001-01-28 15:32:11"),
            jz("2001-01-28 15:32:12"),
            jz("2001-01-28 17:30:11"),
            jz("2001-01-28 17:30:12"),
            jz("2001-01-28 17:32:11"),
            jz("2001-01-28 17:32:12"),
            jz("2001-03-04 15:30:11"),
            jz("2001-03-04 15:30:12"),
            jz("2001-03-04 15:32:11"),
            jz("2001-03-04 15:32:12"),
            jz("2001-03-04 17:30:11"),
            jz("2001-03-04 17:30:12"),
            jz("2001-03-04 17:32:11"),
            jz("2001-03-04 17:32:12"),
            jz("2001-03-25 15:30:11"),
            jz("2001-03-25 15:30:12"),
            jz("2001-03-25 15:32:11"),
            jz("2001-03-25 15:32:12"),
            jz("2001-03-25 17:30:11"),
            jz("2001-03-25 17:30:12"),
            jz("2001-03-25 17:32:11"),
            jz("2001-03-25 17:32:12"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn empty_by_set_pos() {
        assert!(
            target("FREQ=MONTHLY;BYDAY=2TU;BYSETPOS=2", "20150101T001500")
                .next()
                .is_none()
        );
    }

    #[test]
    fn mismatched_dtstart() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYMONTH=4,7", "20180203")
            .take(4)
            .collect();

        let expected = vec![
            jz("2018-02-03"),
            jz("2018-04-03"),
            jz("2018-07-03"),
            jz("2019-04-03"),
        ];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=3", "20180109")
            .take(5)
            .collect();

        let expected = vec![
            // 2018-01-01 counts as BYSETPOS=1
            // 2018-01-08 counts as BYSETPOS=2
            jz("2018-01-09"), // via DTSTART
            jz("2018-01-15"), // via BYSETPOS=3
            jz("2018-02-19"), // via BYSETPOS=3
            jz("2018-03-19"), // via BYSETPOS=3
            jz("2018-04-16"), // via BYSETPOS=3
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn missing_days() {
        let actual: Vec<_> = target("FREQ=MONTHLY", "20180131").take(6).collect();

        let expected = vec![
            jz("2018-01-31"),
            // 2018-02-31 doesn't exist
            jz("2018-03-31"),
            jz("2018-05-31"),
            // 2018-06-31 doesn't exist
            jz("2018-07-31"),
            jz("2018-08-31"),
            // 2018-09-31 doesn't exist
            jz("2018-10-31"),
        ];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=YEARLY", "20120229").take(3).collect();

        let expected = vec![
            jz("2012-02-29"),
            // 2013-02-29 doesn't exist
            // 2014-02-29 doesn't exist
            // 2015-02-29 doesn't exist
            jz("2016-02-29"),
            // 2017-02-29 doesn't exist
            // 2018-02-29 doesn't exist
            // 2019-02-29 doesn't exist
            jz("2020-02-29"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn missing_hours() {
        let actual: Vec<_> = target("FREQ=DAILY;BYHOUR=2", ";TZID=Europe/Warsaw:20250328T023000")
            .take(4)
            .collect();

        let expected = vec![
            jz("2025-03-28T02:30:00+01:00[Europe/Warsaw]"),
            jz("2025-03-29T02:30:00+01:00[Europe/Warsaw]"),
            // 2025-03-30T02:30:00+02:00[Europe/Warsaw] doesn't exist
            jz("2025-03-31T02:30:00+02:00[Europe/Warsaw]"),
            jz("2025-04-01T02:30:00+02:00[Europe/Warsaw]"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn time_travelling() {
        let actual: Vec<_> = target("FREQ=YEARLY;BYMONTH=4,7", "20180505")
            .take(4)
            .collect();

        let expected = vec![
            // no 2018-04-05, because it's earlier than DTSTART
            jz("2018-05-05"),
            jz("2018-07-05"),
            jz("2019-04-05"),
            jz("2019-07-05"),
        ];

        pa::assert_eq!(expected, actual);

        // ---

        let actual: Vec<_> = target("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1", "20180109")
            .take(3)
            .collect();

        let expected = vec![
            // no 2018-01-01, because it's earlier than DTSTART
            jz("2018-01-09"), // via DTSTART
            jz("2018-02-05"), // via BYSETPOS=1
            jz("2018-03-05"), // via BYSETPOS=1
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn dst() {
        // Case: DST transition that happens during the iteration.

        let actual: Vec<_> = target("FREQ=DAILY", ";TZID=Europe/Warsaw:20250328T023000")
            .take(5)
            .collect();

        let expected = vec![
            jz("2025-03-28T02:30:00+01:00[Europe/Warsaw]"),
            jz("2025-03-29T02:30:00+01:00[Europe/Warsaw]"),
            jz("2025-03-30T03:30:00+02:00[Europe/Warsaw]"),
            jz("2025-03-31T02:30:00+02:00[Europe/Warsaw]"),
            jz("2025-04-01T02:30:00+02:00[Europe/Warsaw]"),
        ];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: Iteration that starts on a DST hour.

        let actual: Vec<_> = target("FREQ=DAILY", ";TZID=Europe/Warsaw:20250330T033000")
            .take(3)
            .collect();

        let expected = vec![
            jz("2025-03-30T03:30:00+02:00[Europe/Warsaw]"),
            jz("2025-03-31T03:30:00+02:00[Europe/Warsaw]"),
            jz("2025-04-01T03:30:00+02:00[Europe/Warsaw]"),
        ];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: DST transition that happens during the iteration, over night.
        //
        // This makes sure that we don't do stupid things such as
        // `day.start_of_day() + $hour hours`.
        //
        // start-of-day(2015-10-18) = 2015-10-18 01:00:00, so a naive approach
        // which assumes that start-of-day() always returns 00:00:00 could try
        // to add "extra" hours from the starting date to compensate, and in
        // doing so emit a date-time that's one hour too far.

        let actual: Vec<_> = target("FREQ=DAILY", ";TZID=America/Sao_Paulo:20151016T010000")
            .take(5)
            .collect();

        let expected = vec![
            jz("2015-10-16T01:00:00-03:00[America/Sao_Paulo]"),
            jz("2015-10-17T01:00:00-03:00[America/Sao_Paulo]"),
            jz("2015-10-18T01:00:00-02:00[America/Sao_Paulo]"),
            jz("2015-10-19T01:00:00-02:00[America/Sao_Paulo]"),
            jz("2015-10-20T01:00:00-02:00[America/Sao_Paulo]"),
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn since() {
        // Case: Without BYSETPOS

        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=10,20,30;COUNT=2", "20180101")
            .since(jz("2018-06-01"))
            .take(3)
            .collect();

        let expected = vec![jz("2018-06-10"), jz("2018-06-20"), jz("2018-06-30")];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: With BYSETPOS

        let actual: Vec<_> = target(
            "FREQ=MONTHLY;INTERVAL=3;BYMONTHDAY=10,20,30;BYSETPOS=2;COUNT=2",
            "20180101",
        )
        .since(jz("2018-05-21"))
        .take(3)
        .collect();

        let expected = vec![jz("2018-07-20"), jz("2018-10-20"), jz("2019-01-20")];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: Fast-forwarding right at the next occurrence

        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=10,20,30;COUNT=2", "20180101")
            .since(jz("2018-06-20"))
            .take(3)
            .collect();

        let expected = vec![jz("2018-06-20"), jz("2018-06-30"), jz("2018-07-10")];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: Fast-forwarding before DTSTART

        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=10,20,30;COUNT=2", "20180101")
            .since(jz("2010-01-01"))
            .take(3)
            .collect();

        let expected = vec![jz("2018-01-10"), jz("2018-01-20"), jz("2018-01-30")];

        pa::assert_eq!(expected, actual);

        // ---
        // Case: Fast-forwarding multiple times

        let actual: Vec<_> = target("FREQ=MONTHLY;BYMONTHDAY=10,20,30;COUNT=2", "20180101")
            .since(jz("2010-01-01"))
            .since(jz("2030-04-30"))
            .since(jz("2024-02-03"))
            .take(3)
            .collect();

        let expected = vec![jz("2024-02-10"), jz("2024-02-20"), jz("2024-03-10")];

        pa::assert_eq!(expected, actual);
    }
}
