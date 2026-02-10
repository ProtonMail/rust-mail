use super::*;
use std::iter;
use std::sync::Arc;

/// A distance function, as described in the parent's module top comment.
///
/// Note that for practical reasons, instead of having `fn(Date) -> Span`, we
/// have `fn(Date) -> Date`, like so:
///
/// ```text
/// (weekday-eq Monday 2018-01-01) = 2018-01-01
/// (weekday-eq Tuesday 2018-01-01) = 2018-01-02
/// (weekday-eq Friday 2018-01-01) = 2018-01-05
/// ```
///
/// N.B. this could be modelled as an `enum` instead, but this makes the code
///      actually more difficult to follow
type FactOp = Box<dyn Fn(&JiffZoned) -> Result<JiffZoned, JiffError> + Send + Sync>;

#[derive(Clone)]
pub struct Fact {
    // We want `Self: Clone` so that the underlying iterator is clonable (which
    // simply comes handy) - using `Arc` is the easiest way, considering that we
    // can't have `Box<dyn Fn(...) + Clone>`.
    op: Arc<FactOp>,
}

impl Fact {
    pub fn from_recur(recur: &Recur, start: &JiffZoned) -> Result<Self, JiffError> {
        let instance_of = Self::instance_of(
            start,
            recur.freq,
            i64::from(recur.interval.unwrap_or(1)),
            true,
        )?;

        let op = Self::and(
            iter::once(instance_of)
                .chain(Self::from_recur_filters(recur))
                .chain(Self::from_recur_invariants(recur, start)),
        )
        .unwrap();

        Ok(Self { op: Arc::new(op) })
    }

    /// Creates a constraint for each of the `BY*` parameters.
    fn from_recur_filters(recur: &Recur) -> Option<FactOp> {
        let by_second = Self::or(recur.by_second.iter().map(|second| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            Self::second_eq(second.as_num() as i8)
        }));

        let by_minute = Self::or(recur.by_minute.iter().map(|minute| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            Self::minute_eq(minute.as_num() as i8)
        }));

        let by_hour = Self::or(recur.by_hour.iter().map(|hour| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            Self::hour_eq(hour.as_num() as i8)
        }));

        let by_day = Self::or(recur.by_day.iter().filter_map(|day| {
            match day {
                ByDay::Every(day) => Some(Fact::weekday_eq(day.as_jiff())),

                ByDay::Fixed(nth, day) => {
                    match recur.freq {
                        Freq::Monthly => {
                            Some(Fact::nth_weekday_of_month_eq(nth.get(), day.as_jiff()))
                        }
                        Freq::Yearly => Some(Fact::nth_weekday_of_year_eq(
                            i32::from(nth.get()),
                            day.as_jiff(),
                        )),
                        _ => {
                            // No such thing as "the second Monday this week"
                            // etc.
                            None
                        }
                    }
                }
            }
        }));

        let by_month_day = Self::or(recur.by_month_day.iter().map(|md| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            let day = md.value.as_num() as i8;

            Self::day_eq(if md.sign.is_neg() { -day } else { day })
        }));

        let by_year_day = Self::or(recur.by_year_day.iter().map(|yd| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            let day = yd.value.as_num() as i16;

            Self::day_of_year_eq(if yd.sign.is_neg() { -day } else { day })
        }));

        let by_week_no = if recur.by_week_no.is_empty() {
            None
        } else {
            todo!();
        };

        let by_month = Self::or(recur.by_month.iter().map(|month| {
            #[allow(clippy::cast_possible_wrap, reason = "known to be in range")]
            Self::month_eq(month.as_num() as i8)
        }));

        Self::and(
            by_second
                .into_iter()
                .chain(by_minute)
                .chain(by_hour)
                .chain(by_day)
                .chain(by_month_day)
                .chain(by_year_day)
                .chain(by_week_no)
                .chain(by_month),
        )
    }

    /// Creates a constraint that covers all of the underconstrained parts:
    ///
    /// - `FREQ=MONTHLY;DTSTART=20180114` gets `BYMONTHDAY=14`,
    /// - `FREQ=DAILY;DTSTART=20180101T123456` gets `BYHOUR=12;BYMINUTE=34;BYSECOND=56`,
    /// - etc.
    ///
    /// This is our way of modelling what the RFC means by:
    ///
    /// > Information, not contained in the rule, necessary to determine the
    /// > various recurrence instance start time and dates are derived from
    /// > the Start Time ("DTSTART") component attribute.  For example,
    /// > "FREQ=YEARLY;BYMONTH=1" doesn't specify a specific day within the
    /// > month or a time.  This information would be the same as what is
    /// > specified for "DTSTART".
    fn from_recur_invariants(recur: &Recur, start: &JiffZoned) -> Option<FactOp> {
        let mut facts = Vec::new();

        // ---
        // Constrain the time component

        match recur.freq {
            Freq::Secondly => (),

            Freq::Minutely => {
                if recur.by_second.is_empty() {
                    facts.push(Self::second_eq(start.second()));
                }
            }

            Freq::Hourly => {
                if recur.by_second.is_empty() {
                    facts.push(Self::second_eq(start.second()));
                }
                if recur.by_minute.is_empty() {
                    facts.push(Self::minute_eq(start.minute()));
                }
            }

            Freq::Daily | Freq::Weekly | Freq::Monthly | Freq::Yearly => {
                if recur.by_second.is_empty() {
                    facts.push(Self::second_eq(start.second()));
                }
                if recur.by_minute.is_empty() {
                    facts.push(Self::minute_eq(start.minute()));
                }
                if recur.by_hour.is_empty() {
                    facts.push(Self::observed_hour_eq(start.hour()));
                }
            }
        }

        // ---
        // Constrain the date component

        match recur.freq {
            Freq::Weekly => {
                if recur.by_day.is_empty() {
                    facts.push(Self::weekday_eq(start.weekday()));
                }
            }

            Freq::Monthly => {
                if recur.by_day.is_empty() && recur.by_month_day.is_empty() {
                    facts.push(Self::day_eq(start.day()));
                }
            }

            Freq::Yearly => {
                if recur.by_day.is_empty()
                    && recur.by_month_day.is_empty()
                    && recur.by_year_day.is_empty()
                {
                    facts.push(Self::day_eq(start.day()));
                }

                if recur.by_month.is_empty() && recur.by_year_day.is_empty() {
                    facts.push(Self::month_eq(start.month()));
                }
            }

            _ => (),
        }

        Self::and(facts)
    }

    /// Creates a constraint for date's second; 0..=59.
    ///
    /// E.g. `(second-eq 25)` matches `12:00:25`, `12:01:25` etc.
    fn second_eq(second: i8) -> FactOp {
        Box::new(move |curr| {
            match curr.second().cmp(&second) {
                // E.g. curr=12:30:15 and second=25
                //   -> jump to 12:30:25
                Ordering::Less => curr.with().second(second).build(),

                // E.g. curr=12:30:25 and second=25
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=12:30:30 and second=25
                //   -> jump to 12:31:25
                Ordering::Greater => curr
                    .checked_add(JiffSpan::new().try_minutes(1)?)?
                    .with()
                    .second(second)
                    .build(),
            }
        })
    }

    /// Creates a constraint for date's minute; 0..=59.
    ///
    /// E.g. `(minute-eq 25)` matches `12:25:00`, `13:25:36` etc.
    fn minute_eq(minute: i8) -> FactOp {
        Box::new(move |curr| {
            match curr.minute().cmp(&minute) {
                // E.g. curr=12:15:00 and minute=25
                //   -> jump to 12:25:00
                Ordering::Less => curr.with().minute(minute).second(0).build(),

                // E.g. curr=12:25:00 and minute=25
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=12:30:00 and minute=25
                //   -> jump to 13:25:00
                Ordering::Greater => curr
                    .checked_add(JiffSpan::new().try_hours(1)?)?
                    .with()
                    .minute(minute)
                    .build(),
            }
        })
    }

    /// Creates a constraint for date's hour; 0..=23.
    ///
    /// E.g. `(hour-eq 16)` matches `16:00:00`, `16:25:35` etc.
    fn hour_eq(hour: i8) -> FactOp {
        Box::new(move |curr| {
            match curr.hour().cmp(&hour) {
                // E.g. curr=15:25:35 and hour=16
                //   -> jump to 16:00:00
                Ordering::Less => curr.with().hour(hour).minute(0).second(0).build(),

                // E.g. curr=16:25:35 and hour=16
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=2018-01-01 17:25:35 and hour=16
                //   -> jump to 2018-01-02 16:00:00
                Ordering::Greater => curr
                    .tomorrow()?
                    .with()
                    .hour(hour)
                    .minute(0)
                    .second(0)
                    .build(),
            }
        })
    }

    /// Similar to [`Self::hour_eq()`], but takes into account time zone
    /// transitions.
    fn observed_hour_eq(hour: i8) -> FactOp {
        Box::new(move |curr| {
            // If there's a time zone transition happening between `curr` and
            // `hour`, apply it on the parameter as well.
            //
            // Say we've got curr=2025-03-30 and hour=2.
            //
            // Because on 2025-03-30 we observe a DST transition from 02:00:00
            // to 03:00:00, hour=2 doesn't exist in local time, yielding a
            // literal `curr.hour() == 2` comparison false.
            //
            // That'd be correct in terms of how `BYHOUR` is supposed to work:
            //
            // > Recurrence rules may generate recurrence instances with an invalid
            // > date (e.g., February 30) or nonexistent local time (e.g., 1:30 AM
            // > on a day where the local time is moved forward by an hour at 1:00
            // > AM).  Such recurrence instances MUST be ignored and MUST NOT be
            // > counted as part of the recurrence set.
            //
            // ... but there's one edge case here, the `DTSTART` itself.
            //
            // When a rule is underconstrained:
            //
            //     FREQ=DAILY;DTSTART=20180101T123456
            //
            // ... we insert artificial extra constraints into it:
            //
            //     BYHOUR=12;BYMINUTE=45;BYSECOND=56
            //
            // ... which is our way of modelling what the RFC means by:
            //
            // > Information, not contained in the rule, necessary to determine the
            // > various recurrence instance start time and dates are derived from
            // > the Start Time ("DTSTART") component attribute.  For example,
            // > "FREQ=YEARLY;BYMONTH=1" doesn't specify a specific day within the
            // > month or a time.  This information would be the same as what is
            // > specified for "DTSTART".
            //
            // This is mostly fine, except for cases where DTSTART causes us to
            // iterate over a daylight saving time transition - given:
            //
            //     FREQ=DAILY;DTSTART;TZID=Europe/Warsaw:20250328T023000
            //
            // ... we're expected to emit:
            //
            //     2025-03-29T02:30:00+01:00[Europe/Warsaw]
            //     2025-03-30T03:30:00+02:00[Europe/Warsaw] <!! hour=3 !!>
            //     2025-03-31T02:30:00+02:00[Europe/Warsaw]
            //
            // ... and so we need to distinguish between `BYHOUR` and something
            //     akin to `BYOBSERVEDHOUR`.
            //
            // This is not an edge case explicitly mentioned in the standard,
            // but that's how other systems tend to behave.
            //
            // tl;dr feel free to remove this function and see what breaks, it's
            //       tested

            let next = curr.with().hour(hour).minute(0).second(0).build()?;

            // Same logic as `hour-eq`, just with the remapped `next`
            match curr.hour().cmp(&next.hour()) {
                Ordering::Less => Ok(next),
                Ordering::Equal => Ok(curr.clone()),

                Ordering::Greater => curr
                    .tomorrow()?
                    .with()
                    .hour(hour)
                    .minute(0)
                    .second(0)
                    .build(),
            }
        })
    }

    /// Creates a constraint for date's day of month; -31..=31.
    ///
    /// E.g. `(day-eq 10)` matches `2018-01-10`, `2018-02-10` etc.
    fn day_eq(day: i8) -> FactOp {
        Box::new(move |curr| {
            let abs_day = if day.is_negative() {
                curr.last_of_month()?
                    .checked_sub(JiffSpan::new().try_days(day.abs() - 1)?)?
                    .day()
            } else {
                day
            };

            match curr.day().cmp(&abs_day) {
                // E.g. curr=2018-01-07 and day=10
                //   -> jump to 2018-01-10
                Ordering::Less => curr
                    .checked_add(JiffSpan::new().try_days(abs_day - curr.day())?)?
                    .start_of_day(),

                // E.g. curr=2018-01-10 and day=10
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=2018-01-14 and day=10
                //   -> jump to 2018-02-01
                Ordering::Greater => {
                    if day.is_negative() {
                        let dst = curr
                            .checked_add(JiffSpan::new().try_months(1)?)?
                            .last_of_month()?
                            .checked_sub(JiffSpan::new().try_days(day.abs() - 1)?)?
                            .start_of_day()?;

                        // With a negative day, it's possible that `dst` ends up
                        // before `curr` - for instance given:
                        //
                        // - day=-31
                        // - curr=2018-01-30 12:34:56
                        //
                        // ... we'll end up with `dst = 2018-01-30 00:00:00`.
                        //
                        // We can't return a date that's before `curr` (we're
                        // a positive-only distance function), so let's return
                        // the next best underapproximation - the next month.
                        //
                        // Ideally we'd find the closest month that *does* have
                        // the -31th day, but no need to sweat over such a niche
                        // case.
                        if dst <= *curr {
                            curr.last_of_month()?.tomorrow()?.start_of_day()
                        } else {
                            Ok(dst)
                        }
                    } else {
                        curr.last_of_month()?
                            .checked_add(JiffSpan::new().try_days(day.abs())?)?
                            .start_of_day()
                    }
                }
            }
        })
    }

    /// Creates a constraint for date's month; 1..=12.
    ///
    /// E.g. `(month-eq 4)` matches `2018-04-01`, `2019-04-23` etc.
    fn month_eq(month: i8) -> FactOp {
        Box::new(move |curr| {
            match curr.month().cmp(&month) {
                // E.g. curr=2018-03-14 and month=5
                //   -> jump to 2018-05-01
                Ordering::Less => curr
                    .checked_add(JiffSpan::new().try_months(month - curr.month())?)?
                    .first_of_month()?
                    .start_of_day(),

                // E.g. curr=2018-05-14 and month=5
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=2018-06-14 and month=5
                //   -> jump to 2019-05-01
                Ordering::Greater => curr
                    .checked_add(JiffSpan::new().try_months(12 - curr.month() + month)?)?
                    .first_of_month()?
                    .start_of_day(),
            }
        })
    }

    /// Creates a constraint for date's day of year; -366..=366.
    ///
    /// E.g. `(day-of-year-eq 32)` matches `2018-02-01`, `2019-02-01` etc.
    fn day_of_year_eq(day: i16) -> FactOp {
        Box::new(move |curr| {
            let abs_day = if day.is_negative() {
                curr.last_of_year()?
                    .checked_sub(JiffSpan::new().try_days(day.abs() - 1)?)?
                    .day_of_year()
            } else {
                day
            };

            match curr.day_of_year().cmp(&abs_day) {
                // E.g. curr=2018-01-15 and day=20
                //   -> jump to 2018-01-20
                Ordering::Less => curr
                    .checked_add(JiffSpan::new().try_days(abs_day - curr.day_of_year())?)?
                    .start_of_day(),

                // E.g. curr=2018-01-20 and day=20
                //   -> no-op
                Ordering::Equal => Ok(curr.clone()),

                // E.g. curr=2018-01-21 and day=20
                //   -> jump to 2019-01-20
                Ordering::Greater => {
                    if day.is_negative() {
                        let dst = curr
                            .checked_add(JiffSpan::new().try_years(1)?)?
                            .last_of_year()?
                            .checked_sub(JiffSpan::new().try_days(day.abs() - 1)?)?
                            .start_of_day()?;

                        // With a negative day, it's possible that `dst` ends up
                        // before `curr` - for instance given:
                        //
                        // - day=-366
                        // - curr=2020-12-31 12:34:56
                        //
                        // ... we'll end up with `dst = 2020-12-31 00:00:00`.
                        //
                        // We can't return a date that's before `curr` (we're
                        // a positive-only distance function), so let's return
                        // the next best underapproximation - the next year.
                        //
                        // Ideally we'd find the closest year that *does* have
                        // the -366th day, but no need to sweat over such a
                        // niche case.
                        if dst <= *curr {
                            curr.last_of_year()?.tomorrow()?.start_of_day()
                        } else {
                            Ok(dst)
                        }
                    } else {
                        curr.last_of_year()?
                            .checked_add(JiffSpan::new().try_days(day)?)?
                            .start_of_day()
                    }
                }
            }
        })
    }

    /// Creates a constraint for date's weekday.
    ///
    /// E.g. `(weekday-eq Monday)` matches `2018-01-01`, `2018-01-08` etc.
    fn weekday_eq(wd: JiffWeekday) -> FactOp {
        Box::new(move |curr| {
            if curr.weekday() == wd {
                // E.g. curr=2018-01-01 and wd=Monday
                //   -> no-op
                Ok(curr.clone())
            } else {
                // E.g. curr=2018-01-01 and wd=Wednesday
                //   -> jump to 2018-01-03
                curr.checked_add(JiffSpan::new().try_days(curr.weekday().until(wd))?)?
                    .start_of_day()
            }
        })
    }

    /// Creates a constraint for date to be a specific occurrence of
    /// given weekday across the month.
    ///
    /// E.g. `(nth-weekday-of-month-eq 2 Monday)` matches `2018-01-08` in
    /// January 2018, `2018-02-12` in February 2018 etc.
    fn nth_weekday_of_month_eq(nth: i8, wd: JiffWeekday) -> FactOp {
        Box::new(move |curr| {
            let next = curr.nth_weekday_of_month(nth, wd)?;

            if next.date() == curr.date() {
                // E.g. curr=2018-01-01 and nth=1 and wd=Monday
                //   -> no-op
                Ok(curr.clone())
            } else if next < *curr {
                // E.g. curr=2018-01-07 and nth=1 and wd=Monday
                //   -> jump to 2018-02-05
                curr.checked_add(JiffSpan::new().try_months(1)?)?
                    .nth_weekday_of_month(nth, wd)?
                    .start_of_day()
            } else {
                // E.g. curr=2018-01-01 and nth=2 and wd=Monday
                //   -> jump to 2018-01-08
                next.start_of_day()
            }
        })
    }

    /// Creates a constraint for date to be a specific occurrence of
    /// given weekday across the year.
    ///
    /// E.g. `(nth-weekday-of-year-eq 6 Monday)` matches `2018-02-05` in 2018.
    fn nth_weekday_of_year_eq(nth: i32, wd: JiffWeekday) -> FactOp {
        Box::new(move |curr| {
            let next = curr.nth_weekday(nth, wd)?;

            if next.date() == curr.date() {
                // E.g. curr=2018-01-01 and nth=1 and wd=Monday
                //   -> no-op
                Ok(curr.clone())
            } else if next < *curr {
                // E.g. curr=2018-01-08 and nth=1 and wd=Monday
                //   -> jump to 2019-01-07
                curr.checked_add(JiffSpan::new().try_years(1)?)?
                    .nth_weekday(nth, wd)?
                    .start_of_day()
            } else {
                // E.g. curr=2018-01-01 and nth=2 and wd=Monday
                //   -> jump to 2018-01-08
                next.start_of_day()
            }
        })
    }

    /// Creates a constraint that matches instances (repetitions) of given start
    /// date, frequency, and interval.
    ///
    /// Essentially, we make sure that for some integer `nth` we have:
    ///
    /// ```text
    /// curr = start + freq * interval * nth
    /// ```
    ///
    /// If `inclusive` is true, then an exact repetition will be counted as an
    /// instance - otherwise it will not and the function will return the next
    /// repetition, which comes handy for iterating over the repetitions.
    fn instance_of(
        start: &JiffZoned,
        freq: Freq,
        interval: i64,
        inclusive: bool,
    ) -> Result<FactOp, JiffError> {
        let start = freq.first_of(start)?;

        let unit = match freq {
            Freq::Secondly => JiffUnit::Second,
            Freq::Minutely => JiffUnit::Minute,
            Freq::Hourly => JiffUnit::Hour,
            Freq::Daily => JiffUnit::Day,
            Freq::Weekly => JiffUnit::Week,
            Freq::Monthly => JiffUnit::Month,
            Freq::Yearly => JiffUnit::Year,
        };

        Ok(Box::new(move |curr| {
            let diff = curr
                .since(&start)?
                .round(jiff::SpanRound::new().largest(unit).relative(&start))?;

            match freq {
                Freq::Secondly => {
                    let diff = diff.get_seconds() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_seconds(interval - diff)?)
                    }
                }

                Freq::Minutely => {
                    let diff = diff.get_minutes() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_minutes(interval - diff)?)
                    }
                }

                Freq::Hourly => {
                    let interval = i32::try_from(interval).unwrap_or(1);
                    let diff = diff.get_hours() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_hours(interval - diff)?)
                    }
                }

                Freq::Daily => {
                    let interval = i32::try_from(interval).unwrap_or(1);
                    let diff = diff.get_days() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_days(interval - diff)?)?
                            .start_of_day()
                    }
                }

                Freq::Weekly => {
                    let interval = i32::try_from(interval).unwrap_or(1);
                    let diff = diff.get_weeks() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        let next = curr.checked_add(JiffSpan::new().try_weeks(interval - diff)?)?;

                        if next.weekday() == JiffWeekday::Monday {
                            next.start_of_day()
                        } else {
                            next.nth_weekday(-1, JiffWeekday::Monday)?.start_of_day()
                        }
                    }
                }

                Freq::Monthly => {
                    let interval = i32::try_from(interval).unwrap_or(1);
                    let diff = diff.get_months() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_months(interval - diff)?)?
                            .first_of_month()?
                            .start_of_day()
                    }
                }

                Freq::Yearly => {
                    let interval = i16::try_from(interval).unwrap_or(1);
                    let diff = diff.get_years() % interval;

                    if diff == 0 && inclusive {
                        Ok(curr.clone())
                    } else {
                        curr.checked_add(JiffSpan::new().try_years(interval - diff)?)?
                            .first_of_year()?
                            .start_of_day()
                    }
                }
            }
        }))
    }

    /// Creates a constraint that matches *any* of given constraints.
    ///
    /// E.g. `(or (day-eq 1) (day-eq 2))` matches `2018-01-01`, `2018-01-02`
    /// etc.
    fn or(facts: impl IntoIterator<Item = FactOp>) -> Option<FactOp> {
        let facts: Vec<_> = facts.into_iter().collect();

        if facts.is_empty() {
            None
        } else {
            Some(Box::new(move |curr| {
                let mut best = facts[0](curr)?;

                for fact in facts.iter().skip(1) {
                    best = best.min(fact(curr)?);
                }

                Ok(best)
            }))
        }
    }

    /// Creates a constraint that matches *all* of given constraints.
    ///
    /// E.g. `(and (month-eq 1) (day-eq 2))` matches `2018-01-02`, `2019-01-02`
    /// etc.
    fn and(facts: impl IntoIterator<Item = FactOp>) -> Option<FactOp> {
        let facts: Vec<_> = facts.into_iter().collect();

        if facts.is_empty() {
            None
        } else {
            Some(Box::new(move |curr| {
                let mut best = facts[0](curr)?;

                // Note that we could implement `and` in a similar way to `or`,
                // just with swapped comparison (`.max()` instead of `.min()`),
                // but using composition makes the code ~50% faster.
                //
                // Say we're given:
                //
                // ```
                // (and (month-eq 6) (day-eq 14))
                // ```
                //
                // ... if we now start with `curr = 2018-01-01`, then:
                //
                // - Variant A: max()
                //
                //   (month-eq 2018-01-01 6)
                //   = 2018-06-01
                //
                //   (day-eq 2018-01-01 14)
                //   = 2018-01-14
                //
                //   (max 2018-06-01 2018-01-14)
                //   = 2018-06-01
                //     ^^^^^^^^^^
                //
                //   ^ got a partial answer, needs another iteration to fix the
                //     day - this is correct, but suboptimal
                //
                // - Variant B: composition
                //
                //   (month-eq 2018-01-01 6)
                //   = 2018-06-01
                //
                //   (day-eq 2018-06-01 14)
                //   = 2018-06-14
                //     ^^^^^^^^^^
                //
                //   ^ got a proper answer, no need for another iteration!
                for fact in facts.iter().skip(1) {
                    best = fact(&best)?;
                }

                Ok(best)
            }))
        }
    }

    /// Approximates the next occurence of `self` starting from `curr`.
    ///
    /// This function returns either the exact next occurrence or an
    /// underapproximation of one, it never overshoots.
    ///
    /// The returned date-time is equal to `curr` if `curr` _is_ an occurrence
    /// of `self`, otherwise this function returns a date that's closer to the
    /// actual next occurrence, and you're just supposed to call this function
    /// again until you find its next fixed point.
    pub fn next_occur_of(&self, curr: &JiffZoned) -> Result<JiffZoned, JiffError> {
        (self.op)(curr)
    }

    /// Returns the next instance (repetition) of `curr`.
    pub fn next_instance_of(
        start: &JiffZoned,
        freq: Freq,
        interval: i64,
        curr: &JiffZoned,
    ) -> Result<JiffZoned, JiffError> {
        let fact = Self::instance_of(start, freq, interval, false)?;

        fact(curr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::*;
    use std::borrow::Borrow;

    #[track_caller]
    fn assert(fact: impl Borrow<FactOp>, curr: &str, expected: &str) {
        let curr = jz(curr);
        let expected = jz(expected);
        let actual = fact.borrow()(&curr).unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn second_eq() {
        assert(
            Fact::second_eq(25),
            "2018-01-01 12:30:15",
            "2018-01-01 12:30:25",
        );
        assert(
            Fact::second_eq(25),
            "2018-01-01 12:30:25",
            "2018-01-01 12:30:25",
        );
        assert(
            Fact::second_eq(25),
            "2018-01-01 12:30:30",
            "2018-01-01 12:31:25",
        );
    }

    #[test]
    fn minute_eq() {
        assert(
            Fact::minute_eq(25),
            "2018-01-01 12:15:00",
            "2018-01-01 12:25:00",
        );
        assert(
            Fact::minute_eq(25),
            "2018-01-01 12:25:00",
            "2018-01-01 12:25:00",
        );
        assert(
            Fact::minute_eq(25),
            "2018-01-01 12:30:00",
            "2018-01-01 13:25:00",
        );
    }

    #[test]
    fn hour_eq() {
        assert(
            Fact::hour_eq(16),
            "2018-01-01 15:25:35",
            "2018-01-01 16:00:00",
        );
        assert(
            Fact::hour_eq(16),
            "2018-01-01 16:25:35",
            "2018-01-01 16:25:35",
        );
        assert(
            Fact::hour_eq(16),
            "2018-01-01 17:25:35",
            "2018-01-02 16:00:00",
        );
    }

    #[test]
    fn day_eq() {
        assert(
            Fact::day_eq(10),
            "2018-01-07 12:34:56",
            "2018-01-10 00:00:00",
        );
        assert(
            Fact::day_eq(10),
            "2018-01-10 12:34:56",
            "2018-01-10 12:34:56",
        );
        assert(
            Fact::day_eq(10),
            "2018-01-14 12:34:56",
            "2018-02-10 00:00:00",
        );

        // ---

        assert(
            Fact::day_eq(-1),
            "2018-01-15 12:34:56",
            "2018-01-31 00:00:00",
        );
        assert(
            Fact::day_eq(-10),
            "2018-01-15 12:34:56",
            "2018-01-22 00:00:00",
        );
        assert(
            Fact::day_eq(-20),
            "2018-01-15 12:34:56",
            "2018-02-09 00:00:00",
        );
        assert(
            Fact::day_eq(-31),
            "2018-01-30 12:34:56",
            "2018-02-01 00:00:00",
        );
    }

    #[test]
    fn month_eq() {
        assert(
            Fact::month_eq(5),
            "2018-03-14 12:34:56",
            "2018-05-01 00:00:00",
        );
        assert(
            Fact::month_eq(5),
            "2018-05-14 12:34:56",
            "2018-05-14 12:34:56",
        );
        assert(
            Fact::month_eq(5),
            "2018-06-14 12:34:56",
            "2019-05-01 00:00:00",
        );
    }

    #[test]
    fn day_of_year_eq() {
        assert(
            Fact::day_of_year_eq(20),
            "2018-01-15 12:34:56",
            "2018-01-20 00:00:00",
        );
        assert(
            Fact::day_of_year_eq(20),
            "2018-01-20 12:34:56",
            "2018-01-20 12:34:56",
        );
        assert(
            Fact::day_of_year_eq(20),
            "2018-01-21 12:34:56",
            "2019-01-20 00:00:00",
        );

        // ---

        assert(
            Fact::day_of_year_eq(-1),
            "2018-06-01 12:34:56",
            "2018-12-31 00:00:00",
        );
        assert(
            Fact::day_of_year_eq(-10),
            "2018-06-01 12:34:56",
            "2018-12-22 00:00:00",
        );
        assert(
            Fact::day_of_year_eq(-300),
            "2018-06-01 12:34:56",
            "2019-03-07 00:00:00",
        );
        assert(
            Fact::day_of_year_eq(-366),
            "2020-12-31 12:34:56",
            "2021-01-01 00:00:00",
        );
    }

    #[test]
    fn weekday_eq() {
        assert(
            Fact::weekday_eq(JiffWeekday::Wednesday),
            "2018-01-03 12:34:56",
            "2018-01-03 12:34:56",
        );
        assert(
            Fact::weekday_eq(JiffWeekday::Friday),
            "2018-01-03 12:34:56",
            "2018-01-05 00:00:00",
        );
        assert(
            Fact::weekday_eq(JiffWeekday::Monday),
            "2018-01-03 12:34:56",
            "2018-01-08 00:00:00",
        );
    }

    mod instance_of {
        use super::*;

        #[test]
        fn weekly() {
            let fact =
                Fact::instance_of(&jz("2018-01-03 12:00:00"), Freq::Weekly, 3, true).unwrap();

            // Week #1 = ok
            assert(&fact, "2018-01-01 12:34:56", "2018-01-01 12:34:56");
            assert(&fact, "2018-01-04 12:34:56", "2018-01-04 12:34:56");
            assert(&fact, "2018-01-07 12:34:56", "2018-01-07 12:34:56");

            // Week #2 = jump to week #4
            assert(&fact, "2018-01-08 00:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-08 16:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-11 16:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-14 23:59:59", "2018-01-22 00:00:00");

            // Week #3 = jump to week #4
            assert(&fact, "2018-01-15 16:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-18 16:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-21 16:00:00", "2018-01-22 00:00:00");

            // Week #4 = ok
            assert(&fact, "2018-01-22 00:00:00", "2018-01-22 00:00:00");
            assert(&fact, "2018-01-22 12:34:56", "2018-01-22 12:34:56");
            assert(&fact, "2018-01-25 12:34:56", "2018-01-25 12:34:56");
            assert(&fact, "2018-01-28 12:34:56", "2018-01-28 12:34:56");
        }

        #[test]
        fn monthly() {
            let fact =
                Fact::instance_of(&jz("2018-01-14 12:00:00"), Freq::Monthly, 3, true).unwrap();

            // Month #1 = ok
            assert(&fact, "2018-01-14 12:34:56", "2018-01-14 12:34:56");
            assert(&fact, "2018-01-21 12:34:56", "2018-01-21 12:34:56");
            assert(&fact, "2018-01-31 12:34:56", "2018-01-31 12:34:56");

            // Month #2 = jump to month #4
            assert(&fact, "2018-02-01 00:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-02-01 16:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-02-14 16:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-02-28 23:59:59", "2018-04-01 00:00:00");

            // Month #3 = jump to month #4
            assert(&fact, "2018-03-01 00:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-03-01 16:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-03-14 16:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-03-31 23:59:59", "2018-04-01 00:00:00");

            // Month #4 = ok
            assert(&fact, "2018-04-01 00:00:00", "2018-04-01 00:00:00");
            assert(&fact, "2018-04-01 12:34:56", "2018-04-01 12:34:56");
            assert(&fact, "2018-04-14 12:34:56", "2018-04-14 12:34:56");
            assert(&fact, "2018-04-30 23:59:59", "2018-04-30 23:59:59");
        }

        #[test]
        fn yearly() {
            let fact = Fact::instance_of(&jz("2018-03-07"), Freq::Yearly, 3, true).unwrap();

            assert(&fact, "2018-03-08 16:00:00", "2018-03-08 16:00:00");
            assert(&fact, "2018-06-14 12:34:56", "2018-06-14 12:34:56");
            assert(&fact, "2018-12-31 16:00:00", "2018-12-31 16:00:00");
            assert(&fact, "2019-01-01 16:00:00", "2021-01-01 00:00:00");
        }
    }
}
