use crate::{
    CalendarBootstrapExt, CalendarEventPayloadExt, RsvpAttendee, RsvpCache, RsvpCalendar,
    RsvpError, RsvpEvent, RsvpEventId, RsvpIntent, RsvpOccurrence, RsvpOrganizer, RsvpProgress,
    RsvpRecurrence, RsvpResult,
};
use chrono::DateTime;
use itertools::{Either, Itertools};
use jiff::{Zoned, civil::Weekday};
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent, ProtonCalendar,
};
use proton_core_api::services::proton::Proton;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::CalendarEventDecryptor;
use proton_ical as ical;
use std::{collections::HashMap, num::NonZeroU32};
use tracing::{debug, info, instrument, warn};

pub(super) async fn exec<P>(
    api: &Proton,
    pgp: &P,
    keys: &UnlockedAddressKeys<P>,
    cache: &impl RsvpCache,
    now: &Zoned,
    week_start: Weekday,
    id: &RsvpEventId,
) -> RsvpResult<Option<RsvpEvent>>
where
    P: PGPProviderSync,
{
    let Some((calendar, event)) = fetch(api, cache, id).await? else {
        return Ok(None);
    };

    let decryptor = calendar.create_decryptor(pgp, keys, &event)?;
    let event = extract(pgp, now, week_start, id, calendar, event, &decryptor)?;

    Ok(Some(event))
}

#[instrument(skip_all)]
async fn fetch(
    api: &Proton,
    cache: &impl RsvpCache,
    id: &RsvpEventId,
) -> RsvpResult<Option<(CalendarBootstrap, CalendarEvent)>> {
    info!("Fetching event data");

    let event = match id {
        RsvpEventId::Invite { uid, rid, .. } => {
            let events = api.find_calendar_events(uid, *rid).await?;

            // If this is a repeating event, but we're asking the API without
            // providing the recurrence id - you can imagine we're asking about
            // the "original" event, so to say - the API will return us both the
            // original event and all of its single edits.
            //
            // Since we're interested only in the original event, we can just
            // ignore the single edits and pick the first event from the list
            // (which is guaranteed to be this original we're looking for).
            events.into_iter().next()
        }

        RsvpEventId::Reminder { cal_id, event_id } => {
            Some(api.get_calendar_event(cal_id, event_id).await?)
        }
    };

    if let Some(event) = event {
        info!("Fetching bootstrap data");

        let calendar = cache
            .get_calendar_bootstrap(&event.calendar_id, || {
                // We need for the returned future to be static, otherwise rustc
                // has hard time proving sendness
                let api = api.clone();
                let id = event.calendar_id.clone();

                async move { api.get_calendar_bootstrap(&id).await }
            })
            .await?;

        Ok(Some((calendar, event)))
    } else {
        // Not an error - user might simply have decided to disable the RSVP
        // auto-importing feature.

        Ok(None)
    }
}

fn extract<P>(
    pgp: &P,
    now: &Zoned,
    week_start: Weekday,
    id: &RsvpEventId,
    calendar: CalendarBootstrap,
    event: CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<RsvpEvent>
where
    P: PGPProviderSync,
{
    let meta = extract_metadata(pgp, now, week_start, id, &event, decryptor)?;
    let occurrence = extract_occurrence(&event)?;
    let organizer = extract_organizer(&event)?;
    let attendees = extract_attendees(pgp, &event, decryptor, &organizer)?;
    let calendar = extract_calendar(calendar, &event);

    Ok(RsvpEvent {
        summary: meta.summary,
        location: meta.location,
        description: meta.description,
        recurrence: meta.recurrence,
        occurrence,
        attendees,
        organizer,
        calendar,
        progress: meta.progress,
        intent: meta.intent,
        raw: Box::new(event),
    })
}

fn extract_metadata<P>(
    pgp: &P,
    now: &Zoned,
    week_start: Weekday,
    id: &RsvpEventId,
    event: &CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<Metadata>
where
    P: PGPProviderSync,
{
    debug!("Extracting event's metadata");

    let mut summary = None;
    let mut location = None;
    let mut description = None;
    let mut rrule = None;
    let mut dtstart = None;

    let mut progress = {
        let now = now.timestamp().as_second();

        if now < event.start_time {
            RsvpProgress::Pending
        } else if now < event.end_time {
            RsvpProgress::Ongoing
        } else {
            RsvpProgress::Ended
        }
    };

    // Event data is split between shared events (which contain summary,
    // location and description) and calendar event (which contains the status)
    let events = event
        .shared_events
        .iter()
        .chain(event.calendar_events.iter());

    for event in events {
        let event = event.decrypt_and_parse(pgp, decryptor)?;

        summary = summary.or_else(|| event.summary.map(|sum| sum.value.into_string()));
        location = location.or_else(|| event.location.map(|loc| loc.value.into_string()));

        description =
            description.or_else(|| event.description.map(|desc| desc.value.into_string()));

        rrule = rrule.or(event.rrule);
        dtstart = dtstart.or(event.dtstart);

        if event.status == Some(ical::Status::Cancelled) {
            progress = RsvpProgress::Cancelled;
        }
    }

    let recurrence = rrule
        .zip(dtstart)
        .map(|(rrule, dtstart)| extract_recurrence(&rrule.value, dtstart.value, week_start));

    let intent = match id {
        RsvpEventId::Invite { .. } => RsvpIntent::Invite,
        RsvpEventId::Reminder { .. } => RsvpIntent::Reminder,
    };

    Ok(Metadata {
        summary,
        location,
        description,
        recurrence,
        progress,
        intent,
    })
}

fn extract_recurrence(
    recur: &ical::Recur,
    dtstart: ical::DateOrDt,
    week_start: Weekday,
) -> RsvpRecurrence {
    debug!("Extracting event's recurrence");

    // Sometimes a recurrence rule might be underconstrained - e.g.:
    //
    // ```
    // FREQ=WEEKLY
    // ```
    //
    // ... doesn't really specify which day of week it means.
    //
    // This is legal and in cases like these we're supposed to supplement the
    // rule with information from dtstart, e.g.:
    //
    // ```
    // DTSTART:20180101T120000Z
    // RRULE:FREQ=WEEKLY
    // ```
    //
    // ... means "repeat weekly, starting from 2018-01-01 (Monday)".
    //
    // A recurrence rule might be also fully self-descriptive:
    //
    // ```
    // FREQ=WEEKLY;BYDAY=MO
    // ```
    //
    // In cases like these we don't have to access the dtstart whatsoever, and
    // that's why...
    let dtstart = match Zoned::try_from(dtstart) {
        Ok(dtstart) => Some(dtstart),
        Err(err) => {
            warn!(?err, "Couldn't parse dtstart");

            // ... we don't immediately bail out here - if later it happens that
            // we actually have to access this value, we'll bail out then - but
            // again, there's a chance we won't have to.
            None
        }
    };

    match recur.freq {
        ical::Freq::Daily => extract_recurrence_daily(recur),
        ical::Freq::Weekly => extract_recurrence_weekly(recur, dtstart, week_start),
        ical::Freq::Monthly => extract_recurrence_monthly(recur, dtstart, week_start),
        ical::Freq::Yearly => extract_recurrence_yearly(recur),

        freq => {
            // Most apps, including Proton Calendar, don't allow to create an
            // event that repeats every minute or every hour, so no reason to
            // bother with supporting them here
            RsvpRecurrence::Custom(freq)
        }
    }
}

fn extract_recurrence_daily(recur: &ical::Recur) -> RsvpRecurrence {
    if recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        RsvpRecurrence::EveryDay {
            interval: recur.interval(),
        }
    } else {
        // Too funky, e.g. "every Friday".
        //
        // Not that "every Friday" is too difficult of a rule on its own, no -
        // it's just that those more complicated rules are usually built on top
        // of the weekly or monthly frequency instead:
        //
        // ```
        // // Every Friday
        // FREQ=WEEKLY;BYDAY=FR
        //
        // // Every first Friday of the month
        // FREQ=MONTHLY;BYDAY=+1FR
        // ```
        RsvpRecurrence::Custom(ical::Freq::Daily)
    }
}

fn extract_recurrence_weekly(
    recur: &ical::Recur,
    dtstart: Option<Zoned>,
    week_start: Weekday,
) -> RsvpRecurrence {
    // Case: No constraints whatsoever
    //
    // ```
    // FREQ=WEEKLY
    // ```
    if recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        // If there are no constraints, we take weekday from the dtstart:
        //
        // ```
        // // Every Monday (2018-01-01 is Monday)
        // DTSTART:20180101T120000Z
        // RRULE:FREQ=WEEKLY
        // ```
        let Some(dtstart) = dtstart else {
            return RsvpRecurrence::Custom(ical::Freq::Monthly);
        };

        return RsvpRecurrence::EveryWeekday {
            interval: recur.interval(),
            days: vec![dtstart.weekday()],
        };
    }

    // Case: `BYDAY`
    //
    // ```
    // // Every Monday and Tuesday
    // FREQ=WEEKLY;BYDAY=MO,TU
    // ```
    if !recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        let days = recur
            .by_day
            .iter()
            .filter_map(|day| {
                match day {
                    ical::ByDay::Every(day) => Some(day.as_jiff()),
                    ical::ByDay::Fixed(..) => {
                        // Fixed days have undefined semantics for the weekly
                        // frequency[1], but we can't afford to throw an error
                        // in here.
                        //
                        // No need to log a warning either, since in principle
                        // this should have been already caught and logged by
                        // the ical validator.
                        //
                        // [1] no such thing as "the second Monday this week"
                        None
                    }
                }
            })
            .sorted_by_key(|day| day.since(week_start))
            .collect();

        return RsvpRecurrence::EveryWeekday {
            interval: recur.interval(),
            days,
        };
    }

    // Too funky, e.g. "any Monday on the 42nd week of the year"
    RsvpRecurrence::Custom(ical::Freq::Weekly)
}

#[allow(clippy::too_many_lines)]
fn extract_recurrence_monthly(
    recur: &ical::Recur,
    dtstart: Option<Zoned>,
    week_start: Weekday,
) -> RsvpRecurrence {
    // Case: No constraints whatsoever
    //
    // ```
    // FREQ=MONTHLY
    // ```
    if recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        // If there are no constraints, we take day from the dtstart:
        //
        // ```
        // // Every 1st day of the month
        // DTSTART:20180101T120000Z
        // RRULE:FREQ=MONTHLY
        // ```
        let Some(dtstart) = dtstart else {
            return RsvpRecurrence::Custom(ical::Freq::Monthly);
        };

        // Unwrap-safety: `.day()` returns an integer within `1..=31`
        #[allow(clippy::cast_sign_loss, reason = "returned number is always positive")]
        let day = NonZeroU32::new(dtstart.day() as u32).unwrap();

        return RsvpRecurrence::EveryDayOfMonth {
            interval: recur.interval(),
            days: vec![day],
        };
    }

    // Case: `BYMONTHDAY`
    //
    // ```
    // // Every 10th and 20th day of the month
    // FREQ=MONTHLY;BYMONTHDAY=10,20
    // ```
    if recur.by_day.is_empty()
        && !recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        // `BYMONTHDAY` supports positive and negative constraints:
        //
        // ```
        // // Every 10th, 20th and 30th day of the month
        // BYMONTHDAY=10,20,30
        //
        // // Every last and fifth-to-last day of the month
        // BYMONTHDAY=-1,-5
        //
        // // Every first and last day of the month
        // BYMONTHDAY=1,-1
        // ```
        //
        // Following the example, `10,20,30` would end up in `pos_days` and
        // `-1,-5` would end up in `neg_days`.
        let (neg_days, pos_days): (Vec<_>, Vec<_>) = recur
            .by_month_day
            .iter()
            .filter_map(|day| {
                let sign = day.sign;
                let value = NonZeroU32::new(u32::from(day.value.as_num()))?;

                Some((sign, value))
            })
            .partition_map(|(sign, value)| match sign {
                ical::Sign::Neg => Either::Left(value),
                ical::Sign::Pos => Either::Right(value),
            });

        if !neg_days.is_empty() {
            // Too funky, e.g. "the last day of the month"
            return RsvpRecurrence::Custom(ical::Freq::Monthly);
        }

        return RsvpRecurrence::EveryDayOfMonth {
            interval: recur.interval(),
            days: pos_days.into_iter().sorted().collect(),
        };
    }

    // Case: `BYDAY`, possibly with `BYSETPOS`
    //
    // ```
    // // Every Monday and Friday of the month
    // FREQ=MONTHLY;BYDAY=MO,FR
    //
    // // Every first Monday and second Friday of the month
    // FREQ=MONTHLY;BYDAY=1MO,2FR
    //
    // // Every last Monday of the month
    // FREQ=MONTHLY;BYDAY=-1MO
    //
    // // Every first Monday of the month
    // // (equivalent to `BYDAY=1MO`, some calendars just prefer this form)
    // FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1
    // ```
    if !recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
    {
        // `BYDAY` supports two kinds of constraints:
        //
        // ```
        // // Every Monday and Friday of the month
        // BYDAY=MO,FR
        //
        // // Every second Monday and last Friday of the month
        // BYDAY=+2MO,-1FR
        // ```
        //
        // Following this example, `MO,FR` would land into `every_days` and
        // `+2MO,-1FR` would land into `fixed_days`.
        let (every_days, fixed_days): (Vec<_>, Vec<_>) =
            recur.by_day.iter().partition_map(|day| match day {
                ical::ByDay::Every(day) => Either::Left(day.as_jiff()),
                ical::ByDay::Fixed(nth, day) => Either::Right((*nth, day.as_jiff())),
            });

        // Case: every-days (e.g. `BYDAY=MO,FR`)
        if !every_days.is_empty() && fixed_days.is_empty() && recur.by_set_pos.is_empty() {
            let days = every_days
                .into_iter()
                .sorted_by_key(|day| day.since(week_start))
                .collect();

            return RsvpRecurrence::EveryWeekdayOfMonth {
                interval: recur.interval(),
                days,
            };
        }

        // Case: fixed-days (e.g. `BYDAY=+2MO,-1FR`)
        if every_days.is_empty() && !fixed_days.is_empty() && recur.by_set_pos.is_empty() {
            // Split constraints into negatives (e.g. `-1FR`) and positives
            // (e.g. `+2MO`)
            let (neg_days, pos_days): (Vec<_>, Vec<_>) =
                fixed_days.into_iter().partition_map(|(nth, day)| {
                    let nth_abs = NonZeroU32::from(nth.unsigned_abs());

                    if nth.is_negative() {
                        Either::Left((nth_abs, day))
                    } else {
                        Either::Right((nth_abs, day))
                    }
                });

            match (neg_days.is_empty(), pos_days.is_empty()) {
                // Case: just negative fixed-days (e.g. `-1FR`)
                (false, true) => {
                    if neg_days.iter().any(|(nth, _)| nth.get() != 1) {
                        // Too funky, e.g. "second-to-last Friday of the month"
                        return RsvpRecurrence::Custom(ical::Freq::Monthly);
                    }

                    // Drop `nth`s from days - we know they are all `== 1` here
                    let days = neg_days
                        .into_iter()
                        .map(|(_, day)| day)
                        .sorted_by_key(|day| day.since(week_start))
                        .collect();

                    return RsvpRecurrence::EveryLastWeekdayOfMonth {
                        interval: recur.interval(),
                        days,
                    };
                }

                // Case: just postive fixed-days (e.g. `+2MO`)
                (true, false) => {
                    let days = pos_days
                        .into_iter()
                        .sorted_by_key(|(nth, day)| (*nth, day.since(week_start)))
                        .collect();

                    return RsvpRecurrence::EveryFixedWeekdayOfMonth {
                        interval: recur.interval(),
                        days,
                    };
                }

                // Case: mixed (positive and negative) fixed-days
                _ => {
                    // Too funky, e.g. "every first Friday and every last Sunday
                    // of the month"
                    return RsvpRecurrence::Custom(ical::Freq::Monthly);
                }
            };
        }

        // Case: fixed-days, but faked via every-days paired with `BYSETPOS`
        //
        // ```
        // FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1
        // ```
        //
        // A more canonical form of this expression would be `BYDAY=1MO`, but
        // some calendars (notably Proton Calendar) prefer to use `BYSETPOS`.
        //
        // Note that we do a *very* specific pattern-match in here, because the
        // moment you have two `BYDAY`s:
        //
        // ```
        // FREQ=MONTHLY;BYDAY=MO,TU;BYSETPOS=1
        // ```
        //
        // ... the expression cannot be anymore canonicalized into:
        //
        // ```
        // FREQ=MONTHLY;BYDAY=1MO,1TU
        // ```
        //
        // ... because:
        //
        // - `BYDAY=MO,TU;BYSETPOS=1` = pick the first { Monday or Tuesday },
        // - `BYDAY=1MO,1TU` = pick { the first Monday } *and* { the first Tuesday }.
        //
        // So this really only works for this very specific edge case with one
        // `BYDAY` and one `BYSETPOS`.
        if every_days.len() == 1 && fixed_days.is_empty() && recur.by_set_pos.len() == 1 {
            let day = every_days[0];
            let nth = recur.by_set_pos[0];

            // Unwrap-safety: `.as_num()` returns a non-zero integer
            let nth_num = NonZeroU32::new(u32::from(nth.value.as_num())).unwrap();

            match nth.sign {
                ical::Sign::Neg => {
                    #[allow(clippy::redundant_else)]
                    if nth_num.get() == 1 {
                        return RsvpRecurrence::EveryLastWeekdayOfMonth {
                            interval: recur.interval(),
                            days: vec![day],
                        };
                    } else {
                        // Too funky, e.g. "second-to-last Friday of the month"
                    }
                }

                ical::Sign::Pos => {
                    return RsvpRecurrence::EveryFixedWeekdayOfMonth {
                        interval: recur.interval(),
                        days: vec![(nth_num, day)],
                    };
                }
            }
        }
    }

    // Too funky, e.g. "every Friday the 13th"
    RsvpRecurrence::Custom(ical::Freq::Monthly)
}

fn extract_recurrence_yearly(recur: &ical::Recur) -> RsvpRecurrence {
    if recur.by_day.is_empty()
        && recur.by_month_day.is_empty()
        && recur.by_year_day.is_empty()
        && recur.by_week_no.is_empty()
        && recur.by_month.is_empty()
        && recur.by_set_pos.is_empty()
    {
        RsvpRecurrence::EveryYear {
            interval: recur.interval(),
        }
    } else {
        // Too funky, e.g. "every 10th Monday of the year"
        RsvpRecurrence::Custom(ical::Freq::Yearly)
    }
}

fn extract_occurrence(event: &CalendarEvent) -> RsvpResult<RsvpOccurrence> {
    debug!("Extracting event's occurrence");

    let starts_at = DateTime::from_timestamp(event.start_time, 0)
        .ok_or(RsvpError::EventStartTimeIsOutOfRange)?;

    let ends_at =
        DateTime::from_timestamp(event.end_time, 0).ok_or(RsvpError::EventEndTimeIsOutOfRange)?;

    if event.full_day {
        Ok(RsvpOccurrence::Date {
            starts_at: starts_at.date_naive(),
            ends_at: ends_at.date_naive().pred_opt().unwrap(),
        })
    } else {
        Ok(RsvpOccurrence::DateTime { starts_at, ends_at })
    }
}

fn extract_organizer(event: &CalendarEvent) -> RsvpResult<RsvpOrganizer> {
    // All shared events come from the same author (the event organizer), so
    // let's just pick any and call it a day.
    //
    // Alternatively we could actually go through all of the *.ics payloads and
    // look for `ORGANIZER:...`, but no need to go this crazy for the same piece
    // of information.
    let email = event
        .shared_events
        .first()
        .ok_or(RsvpError::OrganizerIsNotKnown)?
        .author
        .clone();

    Ok(RsvpOrganizer { email })
}

fn extract_attendees<P>(
    pgp: &P,
    event: &CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
    organizer: &RsvpOrganizer,
) -> RsvpResult<Vec<RsvpAttendee>>
where
    P: PGPProviderSync,
{
    debug!("Extracting event's attendees");

    // Attendees are split between `event.attendees` (which contains statuses
    // and ids used by the API) and `event.attendees_event` (which contains
    // just the e-mail addresses and tokens)
    let attendees: HashMap<_, _> = event
        .attendees
        .iter()
        .map(|att| (att.token.as_str(), (&att.id, att.status)))
        .collect();

    let event = event.attendees_event().decrypt_and_parse(pgp, decryptor)?;

    event
        .attendees
        .into_iter()
        .enumerate()
        .filter_map(|(idx, attendee)| {
            debug!(?idx, "Processing attendee");

            extract_attendee(organizer, &attendees, attendee).transpose()
        })
        .collect()
}

fn extract_attendee(
    organizer: &RsvpOrganizer,
    attendees: &HashMap<&str, (&CalendarAttendeeId, CalendarAttendeeStatus)>,
    attendee: ical::Attendee,
) -> RsvpResult<Option<RsvpAttendee>> {
    #[allow(clippy::match_wildcard_for_single_variants)]
    let email = match attendee.address {
        ical::CalAddress::Email(email) => email.into_value().into_string(),
        _ => {
            return Err(RsvpError::AttendeeHasNonEmailAddress);
        }
    };

    // External systems sometimes include organizer as an attendee - in our case
    // though, we split organizer into a different field within the top-level
    // rsvp structure, so if we happen to find attendee-organizer in here, let's
    // remove it to avoid presenting duplicate information
    if email == organizer.email {
        return Ok(None);
    }

    let token = attendee
        .x_pm_token
        .ok_or(RsvpError::AttendeeHasNoXPmToken)?
        .into_string();

    let (id, status) = attendees
        .get(&token.as_str())
        .ok_or(RsvpError::AttendeeIsNotKnown)?;

    Ok(Some(RsvpAttendee {
        id: (*id).clone(),
        email,
        status: *status,
        token: token.into(),
    }))
}

fn extract_calendar(calendar: CalendarBootstrap, event: &CalendarEvent) -> RsvpCalendar {
    let CalendarBootstrap {
        members: [member], ..
    } = calendar;

    RsvpCalendar {
        id: event.calendar_id.clone(),
        name: member.name,
        color: member.color,
    }
}

#[derive(Debug)]
struct Metadata {
    summary: Option<String>,
    location: Option<String>,
    description: Option<String>,
    recurrence: Option<RsvpRecurrence>,
    progress: RsvpProgress,
    intent: RsvpIntent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use proton_calendar_api::{CalendarEventPayload, CalendarEventPayloadType};

    fn event() -> CalendarEvent {
        CalendarEvent {
            shared_events: Vec::default(),
            calendar_events: Vec::default(),
            id: "xxx".into(),
            calendar_id: "xxx".into(),
            start_time: 0,
            end_time: 0,
            full_day: false,
            recurrence_id: None,
            address_key_packet: None,
            shared_key_packet: None,
            attendees_events: [CalendarEventPayload {
                ty: CalendarEventPayloadType::ClearText,
                data: String::default(),
                signature: None,
                author: "spongebob@squarepants.com".into(),
            }],
            attendees: Vec::default(),
            notifications: None,
            color: None,
            is_proton_proton_invite: true,
        }
    }

    #[test]
    fn extract_occurrence_date() {
        let actual = extract_occurrence(&CalendarEvent {
            start_time: 1_745_366_400,
            end_time: 1_745_452_800,
            full_day: true,
            ..event()
        })
        .unwrap();

        let expected = RsvpOccurrence::Date {
            starts_at: NaiveDate::from_ymd_opt(2025, 4, 23).unwrap(),
            ends_at: NaiveDate::from_ymd_opt(2025, 4, 23).unwrap(),
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn extract_occurrence_datetime() {
        let actual = extract_occurrence(&CalendarEvent {
            start_time: 1_528_972_200,
            end_time: 1_528_976_700,
            full_day: false,
            ..event()
        })
        .unwrap();

        let expected = RsvpOccurrence::DateTime {
            starts_at: "2018-06-14T10:30:00Z".parse().unwrap(),
            ends_at: "2018-06-14T11:45:00Z".parse().unwrap(),
        };

        assert_eq!(expected, actual);
    }

    mod extract_recurrence {
        use super::*;
        use ical::IcsRead;
        use test_case::test_case;

        fn num(nth: u32) -> NonZeroU32 {
            NonZeroU32::new(nth).unwrap()
        }

        struct TestCase {
            given_recur: &'static str,
            given_dtstart: &'static str,
            expected: fn() -> RsvpRecurrence,
        }

        // Unsupported - most calendars don't provide secondly events
        const TEST_SECONDLY: TestCase = TestCase {
            given_recur: "FREQ=SECONDLY",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Secondly),
        };

        // Unsupported - most calendars don't provide minutely events
        const TEST_MINUTELY: TestCase = TestCase {
            given_recur: "FREQ=MINUTELY",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Minutely),
        };

        // Unsupported - most calendars don't provide hourly events
        const TEST_HOURLY: TestCase = TestCase {
            given_recur: "FREQ=HOURLY",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Hourly),
        };

        const TEST_DAILY: TestCase = TestCase {
            given_recur: "FREQ=DAILY",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::EveryDay { interval: num(1) },
        };

        const TEST_DAILY_WITH_INTERVAL: TestCase = TestCase {
            given_recur: "FREQ=DAILY;INTERVAL=3",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::EveryDay { interval: num(3) },
        };

        // Unsupported - most clients would use the weekly frequency here
        const TEST_DAILY_WITH_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYDAY=TU",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        // Unsupported - most clients would use the monthly frequency here
        const TEST_DAILY_WITH_BYMONTHDAY: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYMONTHDAY=12",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_DAILY_WITH_BYYEARDAY: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYYEARDAY=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_DAILY_WITH_BYWEEKNO: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYWEEKNO=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_DAILY_WITH_BYMONTH: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYMONTH=6",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        // Unsupported - unspecified semantics
        const TEST_DAILY_WITH_BYSETPOS: TestCase = TestCase {
            given_recur: "FREQ=DAILY;BYSETPOS=1",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Daily),
        };

        const TEST_WEEKLY: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::EveryWeekday {
                interval: num(1),
                days: vec![Weekday::Monday],
            },
        };

        const TEST_WEEKLY_WITH_INTERVAL: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;INTERVAL=3",
            given_dtstart: ":20180102T120000Z",
            expected: || RsvpRecurrence::EveryWeekday {
                interval: num(3),
                days: vec![Weekday::Tuesday],
            },
        };

        const TEST_WEEKLY_WITH_EVERY_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYDAY=FR,SU,MO",
            given_dtstart: ":20180102T120000Z",
            expected: || RsvpRecurrence::EveryWeekday {
                interval: num(1),
                days: vec![Weekday::Monday, Weekday::Friday, Weekday::Sunday],
                // ^ note that days get sorted according to week-start
            },
        };

        // Semi-supported - `+2FR` is spurious, make sure it gets ignored
        const TEST_WEEKLY_WITH_FIXED_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYDAY=SU,+2FR,SA",
            given_dtstart: ":20180102T120000Z",
            expected: || RsvpRecurrence::EveryWeekday {
                interval: num(1),
                days: vec![Weekday::Saturday, Weekday::Sunday],
                // ^ note that days get sorted according to week-start
            },
        };

        // Unsupported - most clients would use the monthly frequency here
        const TEST_WEEKLY_WITH_BYMONTHDAY: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYMONTHDAY=12",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Weekly),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_WEEKLY_WITH_BYYEARDAY: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYYEARDAY=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Weekly),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_WEEKLY_WITH_BYWEEKNO: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYWEEKNO=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Weekly),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_WEEKLY_WITH_BYMONTH: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYMONTH=6",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Weekly),
        };

        // Unsupported - unspecified semantics
        const TEST_WEEKLY_WITH_BYSETPOS: TestCase = TestCase {
            given_recur: "FREQ=WEEKLY;BYSETPOS=1",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Weekly),
        };

        const TEST_MONTHLY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryDayOfMonth {
                interval: num(1),
                days: vec![num(7)],
            },
        };

        const TEST_MONTHLY_WITH_INTERVAL: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;INTERVAL=3",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryDayOfMonth {
                interval: num(3),
                days: vec![num(7)],
            },
        };

        const TEST_MONTHLY_WITH_POSITIVE_BYMONTHDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYMONTHDAY=10,30,20",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryDayOfMonth {
                interval: num(1),
                days: vec![num(10), num(20), num(30)],
                // ^ note that days get sorted according to ordinality
            },
        };

        const TEST_MONTHLY_WITH_NEGATIVE_BYMONTHDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYMONTHDAY=-10,-20,-30",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        const TEST_MONTHLY_WITH_MIXED_BYMONTHDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYMONTHDAY=1,-1",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        const TEST_MONTHLY_WITH_EVERY_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYDAY=FR,TU,MO",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryWeekdayOfMonth {
                interval: num(1),
                days: vec![Weekday::Monday, Weekday::Tuesday, Weekday::Friday],
                // ^ note that days get sorted according to week-start
            },
        };

        const TEST_MONTHLY_WITH_FIXED_POSITIVE_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYDAY=3MO,1TU,2FR,2MO",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryFixedWeekdayOfMonth {
                interval: num(1),
                days: vec![
                    (num(1), Weekday::Tuesday),
                    (num(2), Weekday::Monday),
                    (num(2), Weekday::Friday),
                    (num(3), Weekday::Monday),
                ],
                // ^ note that days get sorted according to ordinality and week_start
            },
        };

        const TEST_MONTHLY_WITH_FIXED_NEGATIVE_BYDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYDAY=-1MO,-1FR,-1TU",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryLastWeekdayOfMonth {
                interval: num(1),
                days: vec![Weekday::Monday, Weekday::Tuesday, Weekday::Friday],
                // ^ note that days get sorted according to week-start
            },
        };

        const TEST_MONTHLY_WITH_BYDAY_AND_POSITIVE_BYSETPOS: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=2",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryFixedWeekdayOfMonth {
                interval: num(1),
                days: vec![(num(2), Weekday::Monday)],
            },
        };

        const TEST_MONTHLY_WITH_BYDAY_AND_NEGATIVE_BYSETPOS: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=-1",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryLastWeekdayOfMonth {
                interval: num(1),
                days: vec![Weekday::Monday],
            },
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_MONTHLY_WITH_BYYEARDAY: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYYEARDAY=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_MONTHLY_WITH_BYWEEKNO: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYWEEKNO=42",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        // Unsupported - most clients would use the yearly frequency here
        const TEST_MONTHLY_WITH_BYMONTH: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYMONTH=6",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        // Unsupported - unspecified semantics
        const TEST_MONTHLY_WITH_BYSETPOS: TestCase = TestCase {
            given_recur: "FREQ=MONTHLY;BYSETPOS=1",
            given_dtstart: ":20180101T120000Z",
            expected: || RsvpRecurrence::Custom(ical::Freq::Monthly),
        };

        const TEST_YEARLY: TestCase = TestCase {
            given_recur: "FREQ=YEARLY",
            given_dtstart: ":20180314T120000Z",
            expected: || RsvpRecurrence::EveryYear { interval: num(1) },
        };

        const TEST_YEARLY_WITH_INTERVAL: TestCase = TestCase {
            given_recur: "FREQ=YEARLY;INTERVAL=3",
            given_dtstart: ":20180107T120000Z",
            expected: || RsvpRecurrence::EveryYear { interval: num(3) },
        };

        #[test_case(TEST_SECONDLY)]
        #[test_case(TEST_MINUTELY)]
        #[test_case(TEST_HOURLY)]
        // ---
        #[test_case(TEST_DAILY)]
        #[test_case(TEST_DAILY_WITH_INTERVAL)]
        #[test_case(TEST_DAILY_WITH_BYDAY)]
        #[test_case(TEST_DAILY_WITH_BYMONTHDAY)]
        #[test_case(TEST_DAILY_WITH_BYYEARDAY)]
        #[test_case(TEST_DAILY_WITH_BYWEEKNO)]
        #[test_case(TEST_DAILY_WITH_BYMONTH)]
        #[test_case(TEST_DAILY_WITH_BYSETPOS)]
        // ---
        #[test_case(TEST_WEEKLY)]
        #[test_case(TEST_WEEKLY_WITH_INTERVAL)]
        #[test_case(TEST_WEEKLY_WITH_EVERY_BYDAY)]
        #[test_case(TEST_WEEKLY_WITH_FIXED_BYDAY)]
        #[test_case(TEST_WEEKLY_WITH_BYMONTHDAY)]
        #[test_case(TEST_WEEKLY_WITH_BYYEARDAY)]
        #[test_case(TEST_WEEKLY_WITH_BYWEEKNO)]
        #[test_case(TEST_WEEKLY_WITH_BYMONTH)]
        #[test_case(TEST_WEEKLY_WITH_BYSETPOS)]
        // ---
        #[test_case(TEST_MONTHLY)]
        #[test_case(TEST_MONTHLY_WITH_INTERVAL)]
        #[test_case(TEST_MONTHLY_WITH_POSITIVE_BYMONTHDAY)]
        #[test_case(TEST_MONTHLY_WITH_NEGATIVE_BYMONTHDAY)]
        #[test_case(TEST_MONTHLY_WITH_MIXED_BYMONTHDAY)]
        #[test_case(TEST_MONTHLY_WITH_EVERY_BYDAY)]
        #[test_case(TEST_MONTHLY_WITH_FIXED_POSITIVE_BYDAY)]
        #[test_case(TEST_MONTHLY_WITH_FIXED_NEGATIVE_BYDAY)]
        #[test_case(TEST_MONTHLY_WITH_BYDAY_AND_POSITIVE_BYSETPOS)]
        #[test_case(TEST_MONTHLY_WITH_BYDAY_AND_NEGATIVE_BYSETPOS)]
        #[test_case(TEST_MONTHLY_WITH_BYYEARDAY)]
        #[test_case(TEST_MONTHLY_WITH_BYWEEKNO)]
        #[test_case(TEST_MONTHLY_WITH_BYMONTH)]
        #[test_case(TEST_MONTHLY_WITH_BYSETPOS)]
        // ---
        #[test_case(TEST_YEARLY)]
        #[test_case(TEST_YEARLY_WITH_INTERVAL)]
        #[allow(clippy::needless_pass_by_value)]
        fn test(case: TestCase) {
            let recur = ical::Recur::from_str(case.given_recur, ical::Value).unwrap();

            let dtstart = ical::DtStart::from_str(case.given_dtstart, ical::Property)
                .unwrap()
                .value;

            let actual = extract_recurrence(&recur, dtstart, Weekday::Monday);

            assert_eq!((case.expected)(), actual);
        }
    }
}
