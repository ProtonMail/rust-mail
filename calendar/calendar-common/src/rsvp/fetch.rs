use crate::{
    CalendarBootstrapExt, CalendarDecryptorKeys, CalendarEventPayloadExt, RsvpAttendee, RsvpCache,
    RsvpCalendar, RsvpContacts, RsvpError, RsvpEvent, RsvpEventId, RsvpFetchApiError,
    RsvpFetchError, RsvpFetchResult, RsvpIntent, RsvpKeys, RsvpOccurrence, RsvpOrganizer,
    RsvpProgress, RsvpRecency, RsvpRecurrence, RsvpRelation, RsvpResult,
};
use derive_more::Debug;
use itertools::{Either, Itertools};
use jiff::{
    Zoned,
    civil::{Date, Weekday},
};
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent, ProtonCalendar,
};
use proton_canonical_email::{self as email, CanonicalEmail};
use proton_core_api::session::Session;
use proton_core_common::validation::is_valid_email_address;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_calendar::CalendarEventDecryptor;
use proton_ical as ical;
use std::{collections::HashMap, num::NonZeroU32};
use tracing::{debug, info, instrument, trace, warn};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run<P, K>(
    api: &Session,
    pgp: &P,
    keys: &K,
    cache: &impl RsvpCache,
    contacts: &impl RsvpContacts,
    now: &Zoned,
    email: &str,
    week_start: Weekday,
    id: &RsvpEventId,
) -> RsvpFetchResult<Option<RsvpEvent>, K>
where
    P: PGPProviderSync,
    K: RsvpKeys,
{
    // Event decryptor as created in the `decrypt()` function borrows the
    // decryption keys, so we need to store them somewhere up the stack compared
    // to the decryption function itself, i.e. here.
    //
    // Ideally we'd include the keys within the `Decrypted` struct, but that'd
    // make it self-referential.
    let mut decryptor_keys = None;

    let state = fetch(api, cache, id).await?;
    let state = decrypt(pgp, keys, state, &mut decryptor_keys).await?;

    trace!("State (decryptable):\n{state:#?}");

    let Some(state) = inflate(pgp, id, state)? else {
        return Ok(None);
    };

    trace!("State (inflated):\n{state:#?}");

    let email = email::canonicalize_auto(email);
    let event = extract(pgp, contacts, now, &email, week_start, id, state).await?;

    trace!("Event:\n{event:#?}");

    Ok(Some(event))
}

#[derive(Debug)]
enum Fetched {
    Some {
        calendar: CalendarBootstrap,
        event: Box<CalendarEvent>,
        children: Vec<CalendarEvent>,
    },

    None(RsvpFetchApiError),
}

#[instrument(skip_all)]
async fn fetch(api: &Session, cache: &impl RsvpCache, id: &RsvpEventId) -> RsvpResult<Fetched> {
    info!("Fetching event data");

    match fetch_ex(api, cache, id).await {
        Err(err) if err.is_network_failure() => {
            warn!(?err, "Got a network failure, trying to continue anyway");

            Ok(Fetched::None(RsvpFetchApiError::NetworkFailure))
        }

        val => val,
    }
}

// Extracted out of `fetch()` since this way it's easier to catch all network
// errors without having to `match` on each `api.whatever()`
async fn fetch_ex(api: &Session, cache: &impl RsvpCache, id: &RsvpEventId) -> RsvpResult<Fetched> {
    let (event, children) = match id {
        RsvpEventId::Invite { uid, rid, .. } => {
            let mut events = api.find_calendar_events(uid, *rid).await?;

            let event = if events.is_empty() {
                None
            } else {
                // If this is a repeating event and we're asking the API without
                // providing any recurrence id, the API will return us both the
                // original event /and/ all of its single edits[1].
                //
                // What we're interested the most is the original event - which
                // is guaranteed to be the first item on the list - but we need
                // to note down the "children-events" as well, because those are
                // needed for the answering functionality.
                //
                // [1] exceptions to the recurrence schedule, like "oh, this one
                //     instance will actually happen on 12:30, not 12:00"
                Some(Box::new(events.remove(0)))
            };

            (event, events)
        }

        RsvpEventId::Reminder { cal_id, event_id } => {
            let event = api.get_calendar_event(cal_id, event_id).await?;

            // Children events are only used by the answering functionality -
            // since you can't answer a reminder, no need to bother fetching
            // children here
            let children = Vec::new();

            (Some(Box::new(event)), children)
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

        Ok(Fetched::Some {
            calendar,
            event,
            children,
        })
    } else {
        // Not an error - event might've been already deleted, user might've
        // decided to disable the RSVP auto-importing feature etc.

        Ok(Fetched::None(RsvpFetchApiError::EventMissing))
    }
}

#[derive(Debug)]
#[debug(bound())]
enum Decryptable<'a, P>
where
    P: PGPProviderSync,
{
    Some {
        decryptor: CalendarEventDecryptor<'a, P>,
        calendar: CalendarBootstrap,
        event: Box<CalendarEvent>,
        children: Vec<CalendarEvent>,
    },

    None(RsvpFetchApiError),
}

#[instrument(skip_all)]
async fn decrypt<'a, P, K>(
    pgp: &P,
    keys: &K,
    state: Fetched,
    decryptor_keys: &'a mut Option<CalendarDecryptorKeys<P>>,
) -> RsvpFetchResult<Decryptable<'a, P>, K>
where
    P: PGPProviderSync,
    K: RsvpKeys,
{
    debug!("Decrypting");

    match state {
        Fetched::Some {
            calendar,
            event,
            children,
        } => {
            let decryptor_keys = decryptor_keys.insert(
                CalendarDecryptorKeys::rsvp(pgp, keys, &calendar, &event)
                    .await
                    .map_err(RsvpFetchError::Keys)?,
            );

            let decryptor = calendar.create_decryptor(pgp, decryptor_keys, &event)?;

            Ok(Decryptable::Some {
                decryptor,
                calendar,
                event,
                children,
            })
        }

        // If we've failed to fetch event from the API, there's nothing to
        // decrypt, so let's just carry the error with us to the next stage
        Fetched::None(err) => Ok(Decryptable::None(err)),
    }
}

#[derive(Debug)]
#[debug(bound())]
struct Inflated<'a, P>
where
    P: PGPProviderSync,
{
    decryptor: Option<CalendarEventDecryptor<'a, P>>,
    calendar: Option<CalendarBootstrap>,
    children: Vec<CalendarEvent>,
    source: Source<'a>,
}

#[instrument(skip_all)]
fn inflate<'a, P>(
    pgp: &P,
    id: &'a RsvpEventId,
    state: Decryptable<'a, P>,
) -> RsvpResult<Option<Inflated<'a, P>>>
where
    P: PGPProviderSync,
{
    debug!("Inflating");

    let (decryptor, calendar, raw, children) = match state {
        Decryptable::Some {
            decryptor,
            calendar,
            event,
            children,
        } => (decryptor, calendar, event, children),

        Decryptable::None(err) => {
            return match id {
                // If we've failed to fetch event from the API, we might still
                // display the widget using data solely from the invitation
                // attachment
                RsvpEventId::Invite { method, invite, .. } => {
                    let source = Source::Invite {
                        raw: None,
                        event: Err(err),
                        method: *method,
                        invite,
                    };

                    Ok(Some(Inflated {
                        decryptor: None,
                        calendar: None,
                        children: Vec::new(),
                        source,
                    }))
                }

                // Reminders are a bit different though - for them we only know
                // their corresponding Proton event id (via email headers), but
                // nothing more.
                //
                // Those headers don't contain the event's title, timestamps
                // etc., so without a network connection there's no way for us
                // to show the widget, i.e. time to give up.
                RsvpEventId::Reminder { .. } => {
                    debug!(
                        "Network seems to be down and there's no way to \
                         continue with just the data at hand - giving up",
                    );

                    Ok(None)
                }
            };
        }
    };

    // When we fetch an event from Proton Calendar, we get a couple of disjoint
    // *.ics payloads - e.g. event summary is kept within a "shared event", but
    // event's status is kept within a "calendar event" (crypto purposes).
    //
    // To make it easier to operate on the event, let's now merge those partial
    // events into one object.
    let event = raw
        .shared_events
        .iter()
        .chain(raw.calendar_events.iter())
        .try_fold(Box::new(ical::VEvent::default()), |mut lhs, rhs| {
            let rhs = rhs.decrypt_and_parse(pgp, &decryptor)?;

            lhs.description = lhs.description.or(rhs.description);
            lhs.dtend = lhs.dtend.or(rhs.dtend);
            lhs.dtstamp = lhs.dtstamp.or(rhs.dtstamp);
            lhs.dtstart = lhs.dtstart.or(rhs.dtstart);
            lhs.location = lhs.location.or(rhs.location);
            lhs.rrule = lhs.rrule.or(rhs.rrule);
            lhs.sequence = lhs.sequence.or(rhs.sequence);
            lhs.status = lhs.status.or(rhs.status);
            lhs.summary = lhs.summary.or(rhs.summary);

            Ok::<_, RsvpError>(lhs)
        })?;

    let source = match id {
        RsvpEventId::Invite { method, invite, .. } => Source::Invite {
            raw: Some(raw),
            event: Ok(event),
            method: *method,
            invite,
        },

        RsvpEventId::Reminder { .. } => Source::Reminder { raw, event },
    };

    Ok(Some(Inflated {
        decryptor: Some(decryptor),
        calendar: Some(calendar),
        children,
        source,
    }))
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip_all)]
async fn extract<P>(
    pgp: &P,
    contacts: &impl RsvpContacts,
    now: &Zoned,
    email: &CanonicalEmail,
    week_start: Weekday,
    id: &RsvpEventId,
    state: Inflated<'_, P>,
) -> RsvpResult<RsvpEvent>
where
    P: PGPProviderSync,
{
    let metadata = extract_metadata(state.source.invite_or_event());
    let recurrence = extract_recurrence(state.source.invite_or_event(), week_start);
    let occurrence = extract_occurrence(state.source.invite_or_event())?;
    let organizer = extract_organizer(contacts, &state.source).await?;

    let attendees = extract_attendees(
        pgp,
        contacts,
        &state.source,
        state.decryptor.as_ref(),
        &organizer,
    )
    .await?;

    let relation = extract_relation(email, id, &organizer, &attendees);
    let calendar = extract_calendar(state.calendar, &state.source);
    let progress = extract_progress(now, &state.source, &occurrence);
    let recency = extract_recency(state.source.invite(), state.source.event());

    let intent = match id {
        RsvpEventId::Invite { .. } => RsvpIntent::Invite,
        RsvpEventId::Reminder { .. } => RsvpIntent::Reminder,
    };

    Ok(RsvpEvent {
        summary: metadata.summary,
        location: metadata.location,
        description: metadata.description,
        recurrence,
        occurrence,
        organizer,
        attendees,
        relation,
        calendar,
        progress,
        recency,
        intent,
        raw: state.source.into_raw_event(),
        children: state.children,
    })
}

fn extract_metadata(event: &ical::VEvent) -> Metadata {
    let summary = event
        .summary
        .as_ref()
        .map(|sum| sum.value.as_str().trim().to_owned())
        .filter(|sum| !sum.is_empty());

    let location = event
        .location
        .as_ref()
        .map(|loc| loc.value.as_str().trim().to_owned())
        .filter(|loc| !loc.is_empty());

    let description = event
        .description
        .as_ref()
        .map(|desc| desc.value.as_str().trim().to_owned())
        .filter(|desc| !desc.is_empty());

    Metadata {
        summary,
        location,
        description,
    }
}

fn extract_recurrence(event: &ical::VEvent, week_start: Weekday) -> Option<RsvpRecurrence> {
    let (Some(rrule), Some(dtstart)) = (&event.rrule, &event.dtstart) else {
        return None;
    };

    let recur = &rrule.value;
    let dtstart = dtstart.value.clone();

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

    Some(match recur.freq {
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
    })
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

fn extract_occurrence(event: &ical::VEvent) -> RsvpResult<RsvpOccurrence> {
    let dtstart = event
        .dtstart
        .as_ref()
        .map(|dtstart| &dtstart.value)
        .ok_or(RsvpError::MissingDtStart)?;

    let dtend = event.dtend.as_ref().map(|dtend| &dtend.value);

    match (dtstart, dtend) {
        (ical::DateOrDt::Date(dtstart), Some(ical::DateOrDt::Date(dtend))) => {
            let starts_at = Date::from(*dtstart);

            // iCal's `DTEND` is exclusive, so e.g. for a single-day event we
            // get:
            //
            // ```
            // DTSTART;VALUE=DATE:20180101
            // DTEND;VALUE=DATE:20180102
            // ```
            //
            // Since working on closed sets is less awkward - in particular it
            // allows you to simply `if DTSTART == DTEND ...` - let's convert
            // this `[DTSTART, DTEND)` into `[DTSTART, DTEND]`:
            let ends_at = Date::from(*dtend).yesterday()?;

            Ok(RsvpOccurrence::Date { starts_at, ends_at })
        }

        (ical::DateOrDt::Date(date), None) => Ok(RsvpOccurrence::Date {
            starts_at: (*date).into(),
            ends_at: (*date).into(),
        }),

        (ical::DateOrDt::DateTime(dtstart), Some(ical::DateOrDt::DateTime(dtend))) => {
            Ok(RsvpOccurrence::DateTime {
                starts_at: Zoned::try_from(dtstart.clone())?,
                ends_at: Zoned::try_from(dtend.clone())?,
            })
        }

        (ical::DateOrDt::DateTime(date), None) => Ok(RsvpOccurrence::DateTime {
            starts_at: Zoned::try_from(date.clone())?,
            ends_at: Zoned::try_from(date.clone())?,
        }),

        _ => Err(RsvpError::MixedDtStartAndDtEnd),
    }
}

async fn extract_organizer(
    contacts: &impl RsvpContacts,
    source: &Source<'_>,
) -> RsvpResult<RsvpOrganizer> {
    // If the invite or event doesn't have any organizer, the user is probably
    // browsing a reminder for a typical, non-invitation'able event.
    //
    // In that case the notion of an organizer is somewhat fuzzy, but to
    // simplify the code downstream let's just say that the user itself is the
    // "organizer" of the event.
    let Some(org) = &source.invite_or_event().organizer else {
        let email = source
            .raw_event()
            .and_then(|event| event.shared_events.first())
            .ok_or_else(|| RsvpError::UnknownOrganizer("missing api event data"))?
            .author
            .clone();

        return Ok(RsvpOrganizer {
            name: contacts.get_display_name(&email).await,
            reply_email: email.clone(),
            display_email: email,
        });
    };

    let ical::CalAddress::Email(org_email) = &org.address else {
        return Err(RsvpError::UnknownOrganizer(
            "organizer doesn't have an email address",
        ));
    };

    let org_email = org_email.value().as_str();
    let reply_email;
    let display_email;

    if let Some(org_alt_email) = &org.email {
        // Some calendar providers - notably Apple - give us two different email
        // addresses to work with, one real and the other one randomized:
        //
        // ```
        // ORGANIZER;CN=Someone;EMAIL=someone@pm.me:mailto:wtzaXCGF@imip.me.com
        //                            ^-----------^        ^------------------^
        // ```
        //
        // In cases like these, `EMAIL` contains the email address we can use to
        // identify the organizer in our contact book, while the other address
        // is where the organizer expects to be replied at.
        reply_email = org_email.to_owned();
        display_email = org_alt_email.value.value().as_str().to_owned();
    } else {
        // Usually though (e.g. for Proton-to-Proton invites) there's going to
        // be just one email address, the same for identifying the organizer and
        // replying to the invite:
        //
        // ```
        // ORGANIZER;CN=Someone;mailto:someone@pm.me
        //                             ^-----------^
        // ```
        //
        // In cases like these, the only thing we're worried about is whether
        // that email address is actually legal (cf. CALWEB-3201) - if it's not,
        // we fall back to sender address from the invitation email message[1].
        //
        // As an extra edge case, if the organizer's email address is not valid,
        // but there's no network connection available (meaning we don't have
        // `source.raw_event()`), we pick this invalid email address anyway.
        //
        // Alternative would be to abort the entire process with an `Err`, which
        // is a bit pity, since we need a "proper" email address only to build
        // the response email, which the user cannot do while being offline.
        //
        // [1] since Proton Calendar automatically imports incoming invites, we
        //     get this information "by proxy" via `SharedEvents[].Author` - we
        //     don't have to actually load the mail message in here
        if is_valid_email_address(org_email) || source.raw_event().is_none() {
            reply_email = org_email.to_owned();
            display_email = org_email.to_owned();
        } else {
            // Unwrap-safety: Guarded above
            reply_email = source
                .raw_event()
                .unwrap()
                .shared_events
                .first()
                .ok_or_else(|| RsvpError::UnknownOrganizer("missing api event data"))?
                .author
                .clone();

            display_email = reply_email.clone();
        }
    }

    Ok(RsvpOrganizer {
        name: contacts.get_display_name(&display_email).await,
        reply_email,
        display_email,
    })
}

async fn extract_attendees<P>(
    pgp: &P,
    contacts: &impl RsvpContacts,
    source: &Source<'_>,
    decryptor: Option<&CalendarEventDecryptor<'_, P>>,
    organizer: &RsvpOrganizer,
) -> RsvpResult<Vec<RsvpAttendee>>
where
    P: PGPProviderSync,
{
    debug!("Extracting event's attendees");

    let attendees = if let (Some(event), Some(decryptor)) = (source.raw_event(), decryptor) {
        extract_attendees_from_event(pgp, contacts, event, decryptor, organizer).await?
    } else {
        extract_attendees_from_invite(contacts, source.invite_or_event()).await
    };

    attendees
        .into_iter()
        .map(sanitize_attendee)
        .map(Ok)
        .collect()
}

async fn extract_attendees_from_event<P>(
    pgp: &P,
    contacts: &impl RsvpContacts,
    event: &CalendarEvent,
    decryptor: &CalendarEventDecryptor<'_, P>,
    organizer: &RsvpOrganizer,
) -> RsvpResult<Vec<RsvpAttendee>>
where
    P: PGPProviderSync,
{
    // Attendees are split between `event.attendees` (which contains statuses
    // and ids used by the API) and `event.attendees_event` (which contains
    // just the e-mail addresses and tokens)
    let attendees_meta: HashMap<_, _> = event
        .attendees
        .iter()
        .map(|att| (att.token.as_str(), (&att.id, att.status)))
        .collect();

    let event_attendees: Vec<_> = event
        .attendees_events
        .iter()
        .map(|event| event.decrypt_and_parse(pgp, decryptor))
        .map_ok(|event| event.attendees.into_iter().enumerate())
        .flatten_ok()
        .collect::<Result<_, _>>()?;

    let mut attendees = Vec::new();

    for (idx, attendee) in event_attendees {
        debug!(?idx, "Processing attendee");

        let attendee =
            extract_attendee_from_event(contacts, organizer, &attendees_meta, attendee).await?;

        if let Some(attendee) = attendee {
            attendees.push(attendee);
        }
    }

    Ok(attendees)
}

async fn extract_attendee_from_event(
    contacts: &impl RsvpContacts,
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
    if email == organizer.reply_email || email == organizer.display_email {
        return Ok(None);
    }

    let token = attendee
        .x_pm_token
        .ok_or(RsvpError::AttendeeHasNoXPmToken)?
        .into_string();

    let (id, status) = attendees
        .get(&token.as_str())
        .ok_or(RsvpError::UnknownAttendee)?;

    Ok(Some(RsvpAttendee {
        id: Some((*id).clone()),
        name: contacts.get_display_name(&email).await,
        email,
        status: Some(*status),
        token: Some(token.into()),
        role: attendee.role.unwrap_or_default(),
    }))
}

async fn extract_attendees_from_invite(
    contacts: &impl RsvpContacts,
    invite: &ical::VEvent,
) -> Vec<RsvpAttendee> {
    let mut attendees = Vec::new();

    for attendee in &invite.attendees {
        if let ical::CalAddress::Email(email) = &attendee.address {
            let email = email.value().as_str();

            attendees.push(RsvpAttendee {
                id: None,
                token: None,
                name: contacts.get_display_name(email).await,
                email: email.into(),
                status: None,
                role: attendee.role.unwrap_or_default(),
            });
        }
    }

    attendees
}

fn sanitize_attendee(mut attendee: RsvpAttendee) -> RsvpAttendee {
    // If the attendee's name is the same as its email, let's drop the name -
    // otherwise the UI is mildly confusing, since you see two equal strings:
    //
    // ```
    // Participants:
    //
    // [ ] someone@pm.me * someone@pm.me
    // ```
    if let Some(name) = &attendee.name {
        if *name == attendee.email {
            attendee.name = None;
        }
    }

    attendee
}

fn extract_calendar(calendar: Option<CalendarBootstrap>, source: &Source) -> Option<RsvpCalendar> {
    let calendar = calendar?;
    let event = source.raw_event()?;

    let CalendarBootstrap {
        members: [member], ..
    } = calendar;

    Some(RsvpCalendar {
        id: event.calendar_id.clone(),
        name: member.name,
        color: member.color,
    })
}

fn extract_progress(now: &Zoned, source: &Source, occurrence: &RsvpOccurrence) -> RsvpProgress {
    if let Ok(event) = source.event() {
        if event.status == Some(ical::Status::Cancelled) {
            return RsvpProgress::Cancelled;
        }
    } else {
        // If the event is not available, it means that either we're offline or
        // the event has been deleted from the user's calendar - in any of those
        // cases there's nothing better we can do beyond checking the invite's
        // `METHOD` property:
        //
        // - `METHOD:REQUEST` = event is (probably) good and alive,
        // - `METHOD:CANCEL` = event is (probably) cancelled.
        //
        // Note that for offline mode this can yield both false-positives and
        // false-negatives - e.g. if you have two emails:
        //
        // - "Invitation for an event starting on ..."
        // - "Cancellation of an event starting on ...".
        //
        // ... then opening the first mail will show the event as still in
        // progress, while opening the second mail will show the event as
        // cancelled - no better solution here, though.
        if let Source::Invite {
            method: ical::Method::Cancel,
            ..
        } = source
        {
            return RsvpProgress::Cancelled;
        }
    }

    // Figuring out whether a recurring event has an instance that happens to
    // be ongoing is somewhat awkward and heavy (cf. RecurIterator), so let's
    // simply report all recurring events as pending.
    //
    // This is slightly invalid - doesn't take into account the `UNTIL` rule, to
    // name a thing - but it's good enough.
    if source.invite_or_event().rrule.is_some() {
        return RsvpProgress::Pending;
    }

    match occurrence {
        RsvpOccurrence::Date { starts_at, ends_at } => {
            if now.date() < *starts_at {
                RsvpProgress::Pending
            } else if now.date() <= *ends_at {
                RsvpProgress::Ongoing
            } else {
                RsvpProgress::Ended
            }
        }

        RsvpOccurrence::DateTime { starts_at, ends_at } => {
            if now < starts_at {
                RsvpProgress::Pending
            } else if now <= ends_at {
                RsvpProgress::Ongoing
            } else {
                RsvpProgress::Ended
            }
        }
    }
}

/// Compares `DTSTAMP` and `SEQUENCE` extracted from `invite.ics` to the event
/// data returned from the API - if there's a mismatch between those, it means
/// that user is looking at an outdated invite and should be warned about this.
fn extract_recency(
    invite: Option<&ical::VEvent>,
    event: Result<&ical::VEvent, RsvpFetchApiError>,
) -> RsvpRecency {
    let Some(invite) = invite else {
        // If there's no invite available, we must be looking at a reminder -
        // those cannot be outdated as we always fetch them fresh from the API.
        return RsvpRecency::Fresh;
    };

    let event = match event {
        Ok(event) => event,
        Err(err) => {
            return RsvpRecency::Unknown(err);
        }
    };

    let invite_dtstamp = invite
        .dtstamp
        .clone()
        .and_then(|dtstamp| Zoned::try_from(dtstamp.value).ok());

    let event_dtstamp = event
        .dtstamp
        .clone()
        .and_then(|dtstamp| Zoned::try_from(dtstamp.value).ok());

    let (Some(invite_dtstamp), Some(event_dtstamp)) = (invite_dtstamp, event_dtstamp) else {
        warn!("Invite and/or event are missing DTSTAMP");

        return RsvpRecency::Fresh;
    };

    let invite_sequence = invite.sequence.map_or(0, |seq| seq.value);
    let event_sequence = event.sequence.map_or(0, |seq| seq.value);

    if invite_dtstamp < event_dtstamp || invite_sequence < event_sequence {
        RsvpRecency::Outdated
    } else {
        RsvpRecency::Fresh
    }
}

fn extract_relation(
    email: &CanonicalEmail,
    id: &RsvpEventId,
    organizer: &RsvpOrganizer,
    attendees: &[RsvpAttendee],
) -> RsvpRelation {
    let attendee_idx = attendees.iter().enumerate().find_map(|(idx, att)| {
        if email::canonicalize_auto(&att.email) == *email {
            Some(idx)
        } else {
            None
        }
    });

    if let Some(attendee_idx) = attendee_idx {
        return RsvpRelation::Attendee { attendee_idx };
    }

    if email::canonicalize_auto(&organizer.reply_email) != *email
        && email::canonicalize_auto(&organizer.display_email) != *email
        && matches!(id, RsvpEventId::Invite { .. })
    {
        RsvpRelation::PartyCrasher
    } else {
        RsvpRelation::Organizer
    }
}

#[derive(Debug)]
enum Source<'a> {
    Invite {
        /// Event data as fetched from Proton Calendar, with raw *.ics payloads,
        /// crypto packets etc.
        ///
        /// This field will be `None` if `event.is_err()`.
        raw: Option<Box<CalendarEvent>>,

        /// Event data as fetched from Proton Calendar, materialized from raw
        /// event data above.
        event: Result<Box<ical::VEvent>, RsvpFetchApiError>,

        /// Method (REQUEST / CANCEL) as extracted from `invite.ics`.
        method: ical::Method,

        /// Event data as parsed from `invite.ics`.
        invite: &'a ical::VEvent,
    },

    Reminder {
        /// Event data as fetched from Proton Calendar, with raw *.ics payloads,
        /// crypto packets etc.
        ///
        /// As compared to [`Source::Invite`], this field is not nullable here -
        /// if there's no internet connection, we bail out early because without
        /// network connection there's nowhere to pull the reminder data out of.
        raw: Box<CalendarEvent>,

        /// Event data as fetched from Proton Calendar, materialized from raw
        /// event data above.
        event: Box<ical::VEvent>,
    },
}

impl Source<'_> {
    fn raw_event(&self) -> Option<&CalendarEvent> {
        match self {
            Source::Invite { raw, .. } => raw.as_deref(),
            Source::Reminder { raw, .. } => Some(raw),
        }
    }

    fn into_raw_event(self) -> Option<Box<CalendarEvent>> {
        match self {
            Source::Invite { raw, .. } => raw,
            Source::Reminder { raw, .. } => Some(raw),
        }
    }

    fn invite(&self) -> Option<&ical::VEvent> {
        match self {
            Source::Invite { invite, .. } => Some(invite),
            Source::Reminder { .. } => None,
        }
    }

    fn event(&self) -> Result<&ical::VEvent, RsvpFetchApiError> {
        match self {
            Source::Invite { event, .. } => match event {
                Ok(event) => Ok(event),
                Err(err) => Err(*err),
            },
            Source::Reminder { event, .. } => Ok(event),
        }
    }

    fn invite_or_event(&self) -> &ical::VEvent {
        match self {
            Source::Invite { invite, .. } => invite,
            Source::Reminder { event, .. } => event,
        }
    }
}

#[derive(Debug, PartialEq)]
struct Metadata {
    summary: Option<String>,
    location: Option<String>,
    description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod extract_progress {
        use super::*;
        use test_case::test_case;

        #[test_case("20180103T120000[UTC]", RsvpProgress::Pending)]
        #[test_case("20180104T120000[UTC]", RsvpProgress::Pending)]
        #[test_case("20180104T235959[UTC]", RsvpProgress::Pending)]
        #[test_case("20180105T000000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180105T120000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180106T120000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180107T120000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180108T120000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180108T235959[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180109T000000[UTC]", RsvpProgress::Ended)]
        #[test_case("20180109T120000[UTC]", RsvpProgress::Ended)]
        #[test_case("20180110T120000[UTC]", RsvpProgress::Ended)]
        fn date(now: &str, expected: RsvpProgress) {
            let now = now.parse().unwrap();
            let invite = ical::VEvent::default();

            let source = Source::Invite {
                raw: None,
                event: Err(RsvpFetchApiError::EventMissing),
                method: ical::Method::Request,
                invite: &invite,
            };

            let occurrence = RsvpOccurrence::Date {
                starts_at: "20180105".parse().unwrap(),
                ends_at: "20180108".parse().unwrap(),
            };

            assert_eq!(expected, extract_progress(&now, &source, &occurrence));
        }

        #[test_case("20180101T100000[UTC]", RsvpProgress::Pending)]
        #[test_case("20180101T115959[UTC]", RsvpProgress::Pending)]
        #[test_case("20180101T120000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180101T130000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180101T125959[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180101T133000[UTC]", RsvpProgress::Ongoing)]
        #[test_case("20180101T133001[UTC]", RsvpProgress::Ended)]
        #[test_case("20180101T140000[UTC]", RsvpProgress::Ended)]
        fn datetime(now: &str, expected: RsvpProgress) {
            let now = now.parse().unwrap();
            let invite = ical::VEvent::default();

            let source = Source::Invite {
                raw: None,
                event: Err(RsvpFetchApiError::EventMissing),
                method: ical::Method::Request,
                invite: &invite,
            };

            let occurrence = RsvpOccurrence::DateTime {
                starts_at: "20180101T120000[UTC]".parse().unwrap(),
                ends_at: "20180101T133000[UTC]".parse().unwrap(),
            };

            assert_eq!(expected, extract_progress(&now, &source, &occurrence));
        }

        #[test_case("20180106T120000[UTC]")]
        #[test_case("20180107T060000[UTC]")]
        #[test_case("20180107T080000[UTC]")]
        #[test_case("20180107T100000[UTC]")]
        #[test_case("20180107T120000[UTC]")]
        #[test_case("20180107T140000[UTC]")]
        #[test_case("20180108T120000[UTC]")]
        fn recurring(now: &str) {
            let now = now.parse().unwrap();

            let invite = ical::VEvent {
                rrule: Some(ical::RRule {
                    value: ical::Recur::new(ical::Freq::Daily),
                }),
                ..ical::VEvent::default()
            };

            let source = Source::Invite {
                raw: None,
                event: Err(RsvpFetchApiError::EventMissing),
                method: ical::Method::Request,
                invite: &invite,
            };

            let occurrence = RsvpOccurrence::DateTime {
                starts_at: "20180107T080000[UTC]".parse().unwrap(),
                ends_at: "20180107T120000[UTC]".parse().unwrap(),
            };

            assert_eq!(
                RsvpProgress::Pending,
                extract_progress(&now, &source, &occurrence),
            );
        }

        #[test]
        fn cancelled() {
            let now = "20180101T120000[UTC]".parse().unwrap();
            let invite = ical::VEvent::default();

            let source = Source::Invite {
                raw: None,
                event: Err(RsvpFetchApiError::EventMissing),
                method: ical::Method::Cancel,
                invite: &invite,
            };

            let occurrence = RsvpOccurrence::DateTime {
                starts_at: "20180101T120000[UTC]".parse().unwrap(),
                ends_at: "20180101T133000[UTC]".parse().unwrap(),
            };

            assert_eq!(
                RsvpProgress::Cancelled,
                extract_progress(&now, &source, &occurrence)
            );
        }
    }

    mod extract_metadata {
        use super::*;
        use proton_ical::ics;
        use test_case::test_case;

        struct TestCase {
            given_event: fn() -> String,
            expected: fn() -> Metadata,
        }

        const TEST_ALL: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY:drinking kool-aid
                    LOCATION:united america of states
                    DESCRIPTION:get a refreshment with me
                "}
            },
            expected: || Metadata {
                summary: Some("drinking kool-aid".into()),
                location: Some("united america of states".into()),
                description: Some("get a refreshment with me".into()),
            },
        };

        const TEST_WHITESPACE: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY: drinking kool-aid
                    LOCATION: united america of states
                    DESCRIPTION: get a refreshment with me
                "}
            },
            expected: || Metadata {
                summary: Some("drinking kool-aid".into()),
                location: Some("united america of states".into()),
                description: Some("get a refreshment with me".into()),
            },
        };

        const TEST_EMPTY_SUMMARY: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY:
                    LOCATION:united america of states
                    DESCRIPTION:get a refreshment with me
                "}
            },
            expected: || Metadata {
                summary: None,
                location: Some("united america of states".into()),
                description: Some("get a refreshment with me".into()),
            },
        };

        const TEST_JUST_SUMMARY: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY:drinking kool-aid
                "}
            },
            expected: || Metadata {
                summary: Some("drinking kool-aid".into()),
                location: None,
                description: None,
            },
        };

        const TEST_EMPTY_LOCATION: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY:drinking kool-aid
                    LOCATION:
                    DESCRIPTION:get a refreshment with me
                "}
            },
            expected: || Metadata {
                summary: Some("drinking kool-aid".into()),
                location: None,
                description: Some("get a refreshment with me".into()),
            },
        };

        const TEST_JUST_LOCATION: TestCase = TestCase {
            given_event: || {
                ics! {"
                    LOCATION:united america of states
                "}
            },
            expected: || Metadata {
                summary: None,
                location: Some("united america of states".into()),
                description: None,
            },
        };

        const TEST_EMPTY_DESCRIPTION: TestCase = TestCase {
            given_event: || {
                ics! {"
                    SUMMARY:drinking kool-aid
                    LOCATION:united america of states
                    DESCRIPTION:
                "}
            },
            expected: || Metadata {
                summary: Some("drinking kool-aid".into()),
                location: Some("united america of states".into()),
                description: None,
            },
        };

        const TEST_JUST_DESCRIPTION: TestCase = TestCase {
            given_event: || {
                ics! {"
                    DESCRIPTION:get a refreshment with me
                "}
            },
            expected: || Metadata {
                summary: None,
                location: None,
                description: Some("get a refreshment with me".into()),
            },
        };

        #[test_case(TEST_ALL)]
        #[test_case(TEST_WHITESPACE)]
        #[test_case(TEST_EMPTY_SUMMARY)]
        #[test_case(TEST_JUST_SUMMARY)]
        #[test_case(TEST_EMPTY_LOCATION)]
        #[test_case(TEST_JUST_LOCATION)]
        #[test_case(TEST_EMPTY_DESCRIPTION)]
        #[test_case(TEST_JUST_DESCRIPTION)]
        #[allow(clippy::needless_pass_by_value)]
        fn test(case: TestCase) {
            let given_event = {
                let src = [
                    "BEGIN:VCALENDAR",
                    "BEGIN:VEVENT",
                    &(case.given_event)(),
                    "END:VEVENT",
                    "END:VCALENDAR",
                ]
                .iter()
                .join("\n");

                ical::VCalendar::from_str(&src)
                    .unwrap()
                    .cal
                    .events
                    .remove(0)
            };

            assert_eq!((case.expected)(), extract_metadata(&given_event));
        }
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
            let event = {
                let rrule = ical::RRule {
                    value: ical::Recur::from_str(case.given_recur, ical::Value).unwrap(),
                };

                let dtstart = ical::DtStart::from_str(case.given_dtstart, ical::Property).unwrap();

                ical::VEvent {
                    rrule: Some(rrule),
                    dtstart: Some(dtstart),
                    ..ical::VEvent::default()
                }
            };

            let actual = extract_recurrence(&event, Weekday::Monday);

            assert_eq!(Some((case.expected)()), actual);
        }
    }

    mod extract_occurrence {
        use super::*;
        use test_case::test_case;

        struct TestCase {
            given_dtstart: fn() -> Option<ical::DateOrDt>,
            given_dtend: fn() -> Option<ical::DateOrDt>,
            expected: fn() -> RsvpResult<RsvpOccurrence>,
        }

        const TEST_DATE: TestCase = TestCase {
            given_dtstart: || Some(ical::DateOrDt::Date(ical::utils::d("20180101"))),
            given_dtend: || Some(ical::DateOrDt::Date(ical::utils::d("20180108"))),
            expected: || {
                Ok(RsvpOccurrence::Date {
                    starts_at: "20180101".parse().unwrap(),
                    ends_at: "20180107".parse().unwrap(),
                })
            },
        };

        const TEST_DATETIME: TestCase = TestCase {
            given_dtstart: || {
                Some(ical::DateOrDt::DateTime(ical::utils::dt(
                    "20180101T123456Z",
                )))
            },
            given_dtend: || {
                Some(ical::DateOrDt::DateTime(ical::utils::dt(
                    "20180108T120000Z",
                )))
            },
            expected: || {
                Ok(RsvpOccurrence::DateTime {
                    starts_at: "20180101T123456[UTC]".parse().unwrap(),
                    ends_at: "20180108T120000Z[UTC]".parse().unwrap(),
                })
            },
        };

        const TEST_MIXED: TestCase = TestCase {
            given_dtstart: || Some(ical::DateOrDt::Date(ical::utils::d("20180101"))),
            given_dtend: || {
                Some(ical::DateOrDt::DateTime(ical::utils::dt(
                    "20180108T120000Z",
                )))
            },
            expected: || Err(RsvpError::MixedDtStartAndDtEnd),
        };

        const TEST_MISSING_DTSTART: TestCase = TestCase {
            given_dtstart: || None,
            given_dtend: || Some(ical::DateOrDt::Date(ical::utils::d("20180108"))),
            expected: || Err(RsvpError::MissingDtStart),
        };

        const TEST_MISSING_DTEND_DATE: TestCase = TestCase {
            given_dtstart: || Some(ical::DateOrDt::Date(ical::utils::d("20180101"))),
            given_dtend: || None,
            expected: || {
                Ok(RsvpOccurrence::Date {
                    starts_at: "20180101".parse().unwrap(),
                    ends_at: "20180101".parse().unwrap(),
                })
            },
        };

        const TEST_MISSING_DTEND_DATETIME: TestCase = TestCase {
            given_dtstart: || {
                Some(ical::DateOrDt::DateTime(ical::utils::dt(
                    "20180101T120000Z",
                )))
            },
            given_dtend: || None,
            expected: || {
                Ok(RsvpOccurrence::DateTime {
                    starts_at: "20180101T120000[UTC]".parse().unwrap(),
                    ends_at: "20180101T120000[UTC]".parse().unwrap(),
                })
            },
        };

        #[test_case(TEST_DATE)]
        #[test_case(TEST_DATETIME)]
        #[test_case(TEST_MIXED)]
        #[test_case(TEST_MISSING_DTSTART)]
        #[test_case(TEST_MISSING_DTEND_DATE)]
        #[test_case(TEST_MISSING_DTEND_DATETIME)]
        #[allow(clippy::needless_pass_by_value)]
        fn test(case: TestCase) {
            let event = {
                let dtstart = (case.given_dtstart)().map(|value| ical::DtStart { value });
                let dtend = (case.given_dtend)().map(|value| ical::DtEnd { value });

                ical::VEvent {
                    dtstart,
                    dtend,
                    ..ical::VEvent::default()
                }
            };

            let expected = (case.expected)().map_err(|err| err.to_string());
            let actual = extract_occurrence(&event).map_err(|err| err.to_string());

            assert_eq!(expected, actual);
        }
    }

    mod extract_recency {
        use super::*;
        use test_case::test_case;

        struct Event {
            dtstamp: Option<&'static str>,
            sequence: Option<u32>,
        }

        impl Event {
            fn build(self) -> ical::VEvent {
                let dtstamp = self.dtstamp.map(|value| ical::DtStamp {
                    value: ical::utils::dt(value),
                });

                let sequence = self.sequence.map(|value| ical::Sequence { value });

                ical::VEvent {
                    dtstamp,
                    sequence,
                    ..ical::VEvent::default()
                }
            }
        }

        struct TestCase {
            given_invite: Option<Event>,
            given_event: Result<Event, RsvpFetchApiError>,
            expected: RsvpRecency,
        }

        const TEST_REMINDER: TestCase = TestCase {
            given_invite: None,
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Fresh,
        };

        const TEST_INVITE_WITH_MATCHING_DTSTAMP_AND_SEQUENCE: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Fresh,
        };

        const TEST_INVITE_WITH_PAST_DTSTAMP: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T100000Z"),
                sequence: Some(3),
            }),
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Outdated,
        };

        const TEST_INVITE_WITH_PAST_SEQUENCE: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(1),
            }),
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Outdated,
        };

        /// From an organizer's point of view, creating an event consists of two
        /// distinct steps: creating an event in the backend and sending out
        /// invites to attendees.
        ///
        /// This requires for the organizer to generate a couple of different
        /// *.ics payloads - once for the purposes of the calendar backend and
        /// then separately for each attendee (for `invite.ics`).
        ///
        /// And unfortunately as of a.d. 2025, each time Proton Calendar has to
        /// generate an *.ics, it puts the client's *current* time into DTSTAMP.
        ///
        /// So when you create an event and invite somebody, what happens is:
        ///
        /// - 12:00:00
        ///   Proton Calendar generates an *.ics payload that describes this new
        ///   event of yours and sends it to the calendar API.
        ///
        /// - 12:00:05 (i.e. a couple of seconds later)
        ///   For each attendee, Proton Calendar generates a new *.ics payload
        ///   that contains the event-invite and dispatches the e-mail with it.
        ///
        /// Since there's two different *.ics paylods involved and they are both
        /// generated using the current-as-of-then time, when you later compare
        /// the `VEVENT` as returned from the API vs `VEVENT` as present inside
        /// `invite.ics`, they will disagree on the `DTSTAMP`.
        ///
        /// API's event will say `DTSTAMP:...120000Z`, while the invite will
        /// contain `DTSTAMP:...120005Z` - i.e. the invite will seem to have
        /// happened in the future!
        ///
        /// This is expected and this test exists to make sure that we check for
        /// outdated invites via `invite_dtstamp < api_dtstamp` instead of, say,
        /// `invite_dtstamp != api_dtstamp`.
        const TEST_INVITE_WITH_FUTURE_DTSTAMP: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120005Z"),
                sequence: Some(3),
            }),
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Fresh,
        };

        const TEST_INVITE_WITH_MISSING_DTSTAMP_1: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: None,
                sequence: None,
            }),
            given_event: Ok(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: Some(3),
            }),
            expected: RsvpRecency::Fresh,
        };

        const TEST_INVITE_WITH_MISSING_DTSTAMP_2: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: None,
            }),
            given_event: Ok(Event {
                dtstamp: None,
                sequence: Some(3),
            }),
            expected: RsvpRecency::Fresh,
        };

        const TEST_MISSING_EVENT: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: None,
            }),
            given_event: Err(RsvpFetchApiError::EventMissing),
            expected: RsvpRecency::Unknown(RsvpFetchApiError::EventMissing),
        };

        const TEST_NETWORK_FAILURE: TestCase = TestCase {
            given_invite: Some(Event {
                dtstamp: Some("20180101T120000Z"),
                sequence: None,
            }),
            given_event: Err(RsvpFetchApiError::NetworkFailure),
            expected: RsvpRecency::Unknown(RsvpFetchApiError::NetworkFailure),
        };

        #[test_case(TEST_REMINDER)]
        #[test_case(TEST_INVITE_WITH_MATCHING_DTSTAMP_AND_SEQUENCE)]
        #[test_case(TEST_INVITE_WITH_PAST_DTSTAMP)]
        #[test_case(TEST_INVITE_WITH_PAST_SEQUENCE)]
        #[test_case(TEST_INVITE_WITH_FUTURE_DTSTAMP)]
        #[test_case(TEST_INVITE_WITH_MISSING_DTSTAMP_1)]
        #[test_case(TEST_INVITE_WITH_MISSING_DTSTAMP_2)]
        #[test_case(TEST_MISSING_EVENT)]
        #[test_case(TEST_NETWORK_FAILURE)]
        fn test(case: TestCase) {
            let invite = case.given_invite.map(Event::build);
            let event = case.given_event.map(Event::build);
            let actual = extract_recency(invite.as_ref(), event.as_ref().map_err(|err| *err));

            assert_eq!(case.expected, actual);
        }
    }
}
