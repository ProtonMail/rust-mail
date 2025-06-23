use super::{RsvpCache, RsvpEventType, RsvpStatus};
use crate::{
    CalendarBootstrapExt, CalendarEventPayloadExt, RsvpAttendee, RsvpCalendar, RsvpError,
    RsvpEvent, RsvpEventId, RsvpOccurrence, RsvpOrganizer, RsvpResult,
};
use chrono::DateTime;
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent, ProtonCalendar,
};
use proton_core_api::services::proton::Proton;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::CalendarEventDecryptor;
use proton_ical as ical;
use std::collections::HashMap;
use tracing::{debug, info, instrument};

pub(super) async fn exec<P>(
    api: &Proton,
    pgp: &P,
    keys: &UnlockedAddressKeys<P>,
    cache: &impl RsvpCache,
    id: &RsvpEventId,
) -> RsvpResult<Option<RsvpEvent>>
where
    P: PGPProviderSync,
{
    let Some((calendar, event)) = fetch(api, cache, id).await? else {
        return Ok(None);
    };

    let decryptor = calendar.create_decryptor(pgp, keys, &event)?;
    let event = extract(pgp, id, calendar, event, &decryptor)?;

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
        RsvpEventId::Direct(cid, eid, _) => Some(api.get_calendar_event(cid, eid).await?),

        RsvpEventId::Indirect(uid, rid) => {
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
    id: &RsvpEventId,
    calendar: CalendarBootstrap,
    event: CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<RsvpEvent>
where
    P: PGPProviderSync,
{
    let ty = match id {
        RsvpEventId::Direct(_, _, ty) => *ty,
        RsvpEventId::Indirect(_, _) => RsvpEventType::Invite,
    };

    let meta = extract_metadata(pgp, &event, decryptor)?;
    let occurrence = extract_occurrence(&event)?;
    let attendees = extract_attendees(pgp, &event, decryptor)?;
    let organizer = extract_organizer(&event)?;
    let calendar = extract_calendar(calendar, &event);

    Ok(RsvpEvent {
        ty,
        summary: meta.summary,
        location: meta.location,
        description: meta.description,
        occurrence,
        attendees,
        organizer,
        calendar,
        status: meta.status,
        raw: Box::new(event),
    })
}

fn extract_metadata<P>(
    pgp: &P,
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
    let mut status = RsvpStatus::Active;

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

        if event.status == Some(ical::Status::Cancelled) {
            status = RsvpStatus::Cancelled;
        }
    }

    Ok(Metadata {
        summary,
        location,
        description,
        status,
    })
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

fn extract_attendees<P>(
    pgp: &P,
    event: &CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
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
        .map(|(idx, attendee)| {
            debug!(?idx, "Processing attendee");

            extract_attendee(&attendees, attendee)
        })
        .collect()
}

fn extract_attendee(
    attendees: &HashMap<&str, (&CalendarAttendeeId, CalendarAttendeeStatus)>,
    attendee: ical::Attendee,
) -> RsvpResult<RsvpAttendee> {
    #[allow(clippy::match_wildcard_for_single_variants)]
    let email = match attendee.address {
        ical::CalAddress::Email(email) => email.into_value().into_string(),
        _ => {
            return Err(RsvpError::AttendeeHasNonEmailAddress);
        }
    };

    let token = attendee
        .x_pm_token
        .ok_or(RsvpError::AttendeeHasNoXPmToken)?
        .into_string();

    let (id, status) = attendees
        .get(&token.as_str())
        .ok_or(RsvpError::AttendeeIsNotKnown)?;

    Ok(RsvpAttendee {
        id: (*id).clone(),
        email,
        status: *status,
        token: token.into(),
    })
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
    status: RsvpStatus,
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
}
