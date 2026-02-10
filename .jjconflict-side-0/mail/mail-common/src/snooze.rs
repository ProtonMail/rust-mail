use chrono::{DateTime, Datelike, Days, NaiveTime, TimeZone, Weekday};
use proton_core_common::{
    datatypes::{UnixTimestamp, WeekStart},
    models::{PaidSubscription, User},
};

/// Snooze options for a given day.
///
/// Options are defined here:
/// <https://protonag.atlassian.net/wiki/spaces/IA/pages/59650963/Snooze+option+availability>
///
#[derive(Debug, Clone, PartialEq)]
pub struct SnoozeOptions {
    pub options: Vec<SnoozeTime>,
    pub show_unsnooze: bool,
}

impl SnoozeOptions {
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::cast_sign_loss)]
    pub fn new<T: TimeZone>(
        today: DateTime<T>,
        week_start: WeekStart,
        user: &User,
        is_snoozed: bool,
    ) -> Option<Self> {
        let nine_am = NaiveTime::from_hms_opt(9, 0, 0)?;
        let weekday = today.weekday();
        let tomorrow = today
            .date_naive()
            .checked_add_days(Days::new(1))?
            .and_time(nine_am);
        let tomorrow = today
            .timezone()
            .from_local_datetime(&tomorrow)
            .single()?
            .into();

        let mut options = vec![SnoozeTime::Tomorrow(tomorrow)];
        let later_this_week = || -> Option<UnixTimestamp> {
            let later_this_week = today
                .date_naive()
                .checked_add_days(Days::new(2))?
                .and_time(nine_am);
            Some(
                today
                    .timezone()
                    .from_local_datetime(&later_this_week)
                    .single()?
                    .into(),
            )
        };

        match weekday {
            Weekday::Mon | Weekday::Tue | Weekday::Wed => {
                let later_this_week = later_this_week()?;
                options.push(SnoozeTime::LaterThisWeek(later_this_week));
            }
            Weekday::Fri if week_start == WeekStart::Monday => {
                let later_this_week = later_this_week()?;
                options.push(SnoozeTime::LaterThisWeek(later_this_week));
            }
            _ => {} // noop
        }

        if week_start != WeekStart::Saturday {
            match weekday {
                Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu => {
                    let this_weekend = today
                        .date_naive()
                        .checked_add_days(Days::new(6 - weekday.number_from_monday() as u64))? // Saturday
                        .and_time(nine_am);
                    let this_weekend = today
                        .timezone()
                        .from_local_datetime(&this_weekend)
                        .single()?
                        .into();
                    options.push(SnoozeTime::ThisWeekend(this_weekend));
                }
                _ => {} // noop: Saturday will be covered by `NextWeek` case
            }
        }

        if weekday != Weekday::Sun {
            // Calculate days to next week's start day
            let days_to_next_week_start = {
                let current_day = weekday.number_from_monday() as i32; // Mon=1, Tue=2, etc.
                let week_start_day = week_start as i32;
                let days_until = (week_start_day + 7 - current_day) % 7;
                if days_until == 0 { 7 } else { days_until } // Always next week, not this week
            };

            if days_to_next_week_start != 1 {
                let next_week = today
                    .date_naive()
                    .checked_add_days(Days::new(days_to_next_week_start as u64))?
                    .and_time(nine_am);
                let next_week = today
                    .timezone()
                    .from_local_datetime(&next_week)
                    .single()?
                    .into();
                options.push(SnoozeTime::NextWeek(next_week));
            } else {
                // noop: Its already covered by `Tomorrow` case
            }
        }

        if user.subscribed.contains(PaidSubscription::MAIL) {
            options.push(SnoozeTime::Custom);
        }

        Some(Self {
            options,
            show_unsnooze: is_snoozed,
        })
    }

    pub fn has_custom_option(&self) -> bool {
        self.options.contains(&SnoozeTime::Custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnoozeTime {
    Tomorrow(UnixTimestamp),
    LaterThisWeek(UnixTimestamp),
    ThisWeekend(UnixTimestamp),
    NextWeek(UnixTimestamp),
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use proton_core_common::models::{PaidSubscription, User};
    use test_case::test_case;

    fn create_test_user(has_mail_subscription: bool) -> User {
        let mut user = User::default();
        if has_mail_subscription {
            user.subscribed = PaidSubscription::MAIL;
        }
        user
    }

    fn parse_timestamp(timestamp_str: &str) -> DateTime<FixedOffset> {
        // Parse the RFC3339/ISO8601 timestamp and convert to local timezone
        DateTime::<FixedOffset>::parse_from_rfc3339(timestamp_str).unwrap()
    }

    // --- Test cases for Monday week start ---
    #[test_case("2025-01-06T12:00:00Z", WeekStart::Monday, false => vec![
        SnoozeTime::Tomorrow(1736240400.into()), // Tuesday 2025-01-07 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1736326800.into()), // Wednesday 2025-01-08 09:00:00 UTC
        SnoozeTime::ThisWeekend(1736586000.into()), // Saturday 2025-01-11 09:00:00 UTC
        SnoozeTime::NextWeek(1736758800.into()) // Monday 2025-01-13 09:00:00 UTC
    ]; "TEST1 - Monday - no subscription")]
    #[test_case("2025-01-06T12:00:00Z", WeekStart::Monday, true => vec![
        SnoozeTime::Tomorrow(1736240400.into()), // Tuesday 2025-01-07 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1736326800.into()), // Wednesday 2025-01-08 09:00:00 UTC
        SnoozeTime::ThisWeekend(1736586000.into()), // Saturday 2025-01-11 09:00:00 UTC
        SnoozeTime::NextWeek(1736758800.into()), // Monday 2025-01-13 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST2 - Monday - with subscription")]
    #[test_case("2025-01-06T12:00:00+01:00", WeekStart::Monday, false => vec![
        SnoozeTime::Tomorrow(1736236800.into()), // Tuesday 2025-01-07 09:00:00 UTC+1 (08:00:00 UTC)
        SnoozeTime::LaterThisWeek(1736323200.into()), // Wednesday 2025-01-08 09:00:00 UTC+1 (08:00:00 UTC)
        SnoozeTime::ThisWeekend(1736582400.into()), // Saturday 2025-01-11 09:00:00 UTC+1 (08:00:00 UTC)
        SnoozeTime::NextWeek(1736755200.into()) // Monday 2025-01-13 09:00:00 UTC+1 (08:00:00 UTC)
    ]; "TEST3 - Monday - Local timezone UTC+1 (8:00:00 UTC)")]
    #[test_case("2025-01-08T20:00:00-05:00", WeekStart::Monday, false => vec![
        SnoozeTime::Tomorrow(1736431200.into()), // Thursday 2025-01-09 09:00:00 EST (14:00:00 UTC)
        SnoozeTime::LaterThisWeek(1736517600.into()), // Friday 2025-01-10 09:00:00 EST (14:00:00 UTC)
        SnoozeTime::ThisWeekend(1736604000.into()), // Saturday 2025-01-11 09:00:00 EST (14:00:00 UTC)
        SnoozeTime::NextWeek(1736776800.into()) // Monday 2025-01-13 09:00:00 EST (14:00:00 UTC)
    ]; "TEST4 - Wednesday EST time, Thursday UTC time")]
    #[test_case("2025-01-09T20:00:00-05:00", WeekStart::Monday, false => vec![
        SnoozeTime::Tomorrow(1736517600.into()), // Friday 2025-01-10 09:00:00 EST (14:00:00 UTC)
        SnoozeTime::ThisWeekend(1736604000.into()), // Saturday 2025-01-11 09:00:00 EST (14:00:00 UTC)
        SnoozeTime::NextWeek(1736776800.into()) // Monday 2025-01-13 09:00:00 EST (14:00:00 UTC)
    ]; "TEST5 - Thursday EST time, Friday UTC time, No LaterThisWeek")]
    #[test_case("2025-01-11T12:34:56Z", WeekStart::Monday, false => vec![
        SnoozeTime::Tomorrow(1736672400.into()), // Sunday 2025-01-12 09:00:00 UTC
        SnoozeTime::NextWeek(1736758800.into()) // Monday 2025-01-13 09:00:00 UTC
    ]; "TEST6 - Saturday UTC time, No LaterThisWeek and ThisWeekend")]
    #[test_case("2025-01-12T12:34:56Z", WeekStart::Monday, true => vec![
        SnoozeTime::Tomorrow(1736758800.into()), // Monday 2025-01-13 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST7 - Sunday UTC time, With subscription, No LaterThisWeek and ThisWeekend and NextWeek")]
    // --- Test cases for Sunday week start ---
    #[test_case("2025-01-12T12:00:00Z", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1736758800.into()), // Monday 2025-01-13 09:00:00 UTC
    ]; "TEST8 - Sunday start of week, no subscription, only Tomorrow")]
    #[test_case("2025-01-12T12:00:00Z", WeekStart::Sunday, true => vec![
        SnoozeTime::Tomorrow(1736758800.into()), // Monday 2025-01-13 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST9 - Sunday start of week, with subscription")]
    #[test_case("2025-01-13T12:00:00Z", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1736845200.into()), // Tuesday 2025-01-14 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1736931600.into()), // Wednesday 2025-01-15 09:00:00 UTC
        SnoozeTime::ThisWeekend(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::NextWeek(1737277200.into()) // Sunday 2025-01-19 09:00:00 UTC
    ]; "TEST10 - Monday when week starts Sunday, no subscription")]
    #[test_case("2025-01-13T12:00:00Z", WeekStart::Sunday, true => vec![
        SnoozeTime::Tomorrow(1736845200.into()), // Tuesday 2025-01-14 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1736931600.into()), // Wednesday 2025-01-15 09:00:00 UTC
        SnoozeTime::ThisWeekend(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::NextWeek(1737277200.into()), // Sunday 2025-01-19 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST11 - Monday when week starts Sunday, with subscription")]
    #[test_case("2025-01-15T12:00:00Z", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1737018000.into()), // Thursday 2025-01-16 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1737104400.into()), // Friday 2025-01-17 09:00:00 UTC
        SnoozeTime::ThisWeekend(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::NextWeek(1737277200.into()) // Sunday 2025-01-19 09:00:00 UTC
    ]; "TEST12 - Wednesday when week starts Sunday, no subscription")]
    #[test_case("2025-01-16T12:00:00Z", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1737104400.into()), // Friday 2025-01-17 09:00:00 UTC
        SnoozeTime::ThisWeekend(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::NextWeek(1737277200.into()) // Sunday 2025-01-19 09:00:00 UTC
    ]; "TEST13 - Thursday when week starts Sunday, no subscription")]
    #[test_case("2025-01-16T15:30:45+02:00", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1737097200.into()), // Friday 2025-01-17 09:00:00 UTC+2 (07:00:00 UTC)
        SnoozeTime::ThisWeekend(1737183600.into()), // Saturday 2025-01-18 09:00:00 UTC+2 (07:00:00 UTC)
        SnoozeTime::NextWeek(1737270000.into()) // Sunday 2025-01-19 09:00:00 UTC+2 (07:00:00 UTC)
    ]; "TEST14 - Thursday UTC+2 when week starts Sunday, no LaterThisWeek")]
    #[test_case("2025-01-17T08:15:30-08:00", WeekStart::Sunday, true => vec![
        SnoozeTime::Tomorrow(1737219600.into()), // Saturday 2025-01-18 09:00:00 PST (17:00:00 UTC)
        SnoozeTime::NextWeek(1737306000.into()), // Sunday 2025-01-19 09:00:00 PST (17:00:00 UTC)
        SnoozeTime::Custom
    ]; "TEST15 - Friday PST when week starts Sunday, with subscription")]
    #[test_case("2025-01-18T22:45:10+09:00", WeekStart::Sunday, false => vec![
        SnoozeTime::Tomorrow(1737244800.into()), // Sunday 2025-01-19 09:00:00 JST (00:00:00 UTC)
    ]; "TEST16 - Saturday JST when week starts Sunday, no subscription")]
    // --- Test cases for Saturday week start ---
    #[test_case("2025-01-11T12:00:00Z", WeekStart::Saturday, false => vec![
        SnoozeTime::Tomorrow(1736672400.into()), // Sunday 2025-01-12 09:00:00 UTC
        SnoozeTime::NextWeek(1737190800.into()) // Saturday 2025-01-18 09:00:00 UTC
    ]; "TEST17 - Saturday start of week, no subscription, only Tomorrow")]
    #[test_case("2025-01-11T12:00:00Z", WeekStart::Saturday, true => vec![
        SnoozeTime::Tomorrow(1736672400.into()), // Sunday 2025-01-12 09:00:00 UTC
        SnoozeTime::NextWeek(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST18 - Saturday start of week, with subscription")]
    #[test_case("2025-01-12T12:00:00Z", WeekStart::Saturday, false => vec![
        SnoozeTime::Tomorrow(1736758800.into()), // Monday 2025-01-13 09:00:00 UTC
    ]; "TEST19 - Sunday when week starts Saturday, no subscription")]
    #[test_case("2025-01-13T12:00:00Z", WeekStart::Saturday, false => vec![
        SnoozeTime::Tomorrow(1736845200.into()), // Tuesday 2025-01-14 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1736931600.into()), // Wednesday 2025-01-15 09:00:00 UTC
        SnoozeTime::NextWeek(1737190800.into()) // Saturday 2025-01-18 09:00:00 UTC
    ]; "TEST20 - Monday when week starts Saturday, no subscription")]
    #[test_case("2025-01-14T12:00:00Z", WeekStart::Saturday, true => vec![
        SnoozeTime::Tomorrow(1736931600.into()), // Wednesday 2025-01-15 09:00:00 UTC
        SnoozeTime::LaterThisWeek(1737018000.into()), // Thursday 2025-01-16 09:00:00 UTC
        SnoozeTime::NextWeek(1737190800.into()), // Saturday 2025-01-18 09:00:00 UTC
        SnoozeTime::Custom
    ]; "TEST21 - Tuesday when week starts Saturday, with subscription")]
    #[test_case("2025-01-16T12:00:00Z", WeekStart::Saturday, false => vec![
        SnoozeTime::Tomorrow(1737104400.into()), // Friday 2025-01-17 09:00:00 UTC
        SnoozeTime::NextWeek(1737190800.into()) // Saturday 2025-01-18 09:00:00 UTC
    ]; "TEST22 - Thursday when week starts Saturday, no subscription")]
    #[test_case("2025-01-16T18:45:00+02:00", WeekStart::Saturday, false => vec![
        SnoozeTime::Tomorrow(1737097200.into()), // Friday 2025-01-17 09:00:00 EET (07:00:00 UTC)
        SnoozeTime::NextWeek(1737183600.into()) // Saturday 2025-01-18 09:00:00 EET (07:00:00 UTC)
    ]; "TEST23 - Thursday EET when week starts Saturday, no LaterThisWeek")]
    #[test_case("2025-01-17T14:20:00-06:00", WeekStart::Saturday, true => vec![
        SnoozeTime::Tomorrow(1737212400.into()), // Saturday 2025-01-18 09:00:00 CST (15:00:00 UTC)
        SnoozeTime::Custom
    ]; "TEST24 - Friday CST when week starts Saturday, with subscription")]
    fn test_snooze_options(
        timestamp_str: &str,
        week_start: WeekStart,
        has_mail_subscription: bool,
    ) -> Vec<SnoozeTime> {
        let today = parse_timestamp(timestamp_str);
        let user = create_test_user(has_mail_subscription);

        SnoozeOptions::new(today, week_start, &user, false)
            .unwrap()
            .options
    }
}
