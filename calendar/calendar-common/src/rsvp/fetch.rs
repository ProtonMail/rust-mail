use crate::{RsvpAttendee, RsvpCalendar, RsvpEvent, RsvpEventId, RsvpOccurrence, RsvpResult};
use chrono::DateTime;
use proton_calendar_api::{
    CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent, CalendarEventPayload, ProtonCalendar,
};
use proton_core_api::services::proton::Proton;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::{
    CalendarEventDecryptor, EncryptedIcsRef, KeyPacketRef, KeyPackets, LockedCalendarKey,
};
use proton_ical as ical;
use std::{borrow::Cow, collections::HashMap};
use tracing::{debug, info};

use super::RsvpError;

pub(super) async fn fetch<P>(
    api: &Proton,
    pgp: &P,
    keys: &UnlockedAddressKeys<P>,
    event: &RsvpEventId,
) -> RsvpResult<Option<RsvpEvent>>
where
    P: PGPProviderSync,
{
    let Some((calendar, event)) = fetch_encrypted(api, event).await? else {
        return Ok(None);
    };

    let decryptor = create_decryptor(pgp, keys, &calendar, &event)?;
    let event = extract(pgp, calendar, &event, &decryptor)?;

    Ok(Some(event))
}

async fn fetch_encrypted(
    api: &Proton,
    event: &RsvpEventId,
) -> RsvpResult<Option<(CalendarBootstrap, CalendarEvent)>> {
    info!("Fetching event data");

    let event = api
        .get_calendar_event(&event.uid, event.recurrence_id.as_ref())
        .await?;

    if let Some(event) = event {
        info!("Fetching bootstrap data");

        let calendar = api.get_calendar_bootstrap(&event.calendar_id).await?;

        Ok(Some((calendar, event)))
    } else {
        // Not an error - user might simply have decided to disable the RSVP
        // auto-importing feature.

        Ok(None)
    }
}

fn create_decryptor<'a, P>(
    pgp: &'a P,
    address_keys: &'a UnlockedAddressKeys<P>,
    calendar: &CalendarBootstrap,
    event: &CalendarEvent,
) -> RsvpResult<CalendarEventDecryptor<'a, P>>
where
    P: PGPProviderSync,
{
    let calendar_key = LockedCalendarKey::from_bootstrap(calendar)?.import(pgp, address_keys)?;

    let key_packets = {
        let address_key_packet = event
            .address_key_packet
            .as_deref()
            .map(KeyPacketRef::from_base64);

        let shared_key_packet = event
            .shared_key_packet
            .as_deref()
            .map(KeyPacketRef::from_base64);

        KeyPackets {
            address_key_packet,
            shared_key_packet,
        }
    };

    CalendarEventDecryptor::new(pgp, address_keys, &calendar_key, key_packets).map_err(Into::into)
}

fn extract<P>(
    pgp: &P,
    calendar: CalendarBootstrap,
    event: &CalendarEvent,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<RsvpEvent>
where
    P: PGPProviderSync,
{
    let meta = extract_metadata(pgp, event, decryptor)?;
    let occurrence = extract_occurrence(event)?;
    let attendees = extract_attendees(pgp, event, decryptor)?;
    let calendar = extract_calendar(calendar);

    Ok(RsvpEvent {
        summary: meta.summary,
        location: meta.location,
        description: meta.description,
        occurrence,
        attendees,
        calendar,
    })
}

fn decrypt_and_parse<P>(
    pgp: &P,
    event: &CalendarEventPayload,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<ical::VEvent>
where
    P: PGPProviderSync,
{
    let ics = if event.ty.is_encrypted() {
        let ics = EncryptedIcsRef::from_base64(&event.data);
        let ics = decryptor.decrypt(pgp, ics, None)?.into_bytes();

        Cow::Owned(ics)
    } else {
        let ics = event.data.as_bytes();

        Cow::Borrowed(ics)
    };

    let out = ical::VCalendar::from_bytes(&ics)?;

    // Since clients are not necessarily 100% RFC-compliant, it's expected that
    // we'll get some parser or validator messages here.
    //
    // Those messages are not errors per se, because if we got to this point, we
    // were able to successfully recover some useful information from the *.ics,
    // so there's no point in bailing out with an error.
    for msg in out.msgs {
        debug!("ics-parser said: {msg}");
    }
    for viol in out.viols {
        debug!("ics-validator said: {viol}");
    }

    let cal = out.cal;

    if cal.events.len() > 1 {
        return Err(RsvpError::IcsContainsMoreThanOneEvent);
    }

    cal.events
        .into_iter()
        .next()
        .ok_or(RsvpError::IcsContainsNoEvents)
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

    let event = event
        .shared_events
        .iter()
        .find(|event| event.ty.is_encrypted())
        .ok_or(RsvpError::CouldntFindSharedEvent)?;

    let event = decrypt_and_parse(pgp, event, decryptor)?;

    let summary = event
        .summary
        .ok_or(RsvpError::IcsEventHasNoSummary)?
        .value
        .into_string();

    let location = event.location.map(|loc| loc.value.into_string());
    let description = event.description.map(|desc| desc.value.into_string());

    Ok(Metadata {
        summary,
        location,
        description,
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

    // Attendees data are split between `event.attendees` (which contains
    // just the rsvp statuses) and `event.attendees_events` (which contains
    // just the e-mail addresses and tokens), so we must join both
    let statuses: HashMap<_, _> = event
        .attendees
        .iter()
        .map(|att| (att.token.as_str(), att.status))
        .collect();

    let mut attendees = Vec::new();

    for (idx, event) in event.attendees_events.iter().enumerate() {
        debug!(?idx, "Processing attendee event");

        let event = decrypt_and_parse(pgp, event, decryptor)?;

        for (aidx, attendee) in event.attendees.into_iter().enumerate() {
            debug!(?aidx, "Processing attendee");

            attendees.push(map_attendee(&statuses, attendee)?);
        }
    }

    Ok(attendees)
}

fn map_attendee(
    statuses: &HashMap<&str, CalendarAttendeeStatus>,
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

    let status = *statuses
        .get(&token.as_str())
        .ok_or(RsvpError::AttendeeHasUnknownStatus)?;

    Ok(RsvpAttendee { email, status })
}

fn extract_calendar(cal: CalendarBootstrap) -> RsvpCalendar {
    let cal = cal.into_member();

    RsvpCalendar {
        name: cal.name,
        color: cal.color,
    }
}

#[derive(Debug)]
struct Metadata {
    summary: String,
    location: Option<String>,
    description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn extract_occurrence_date() {
        let actual = extract_occurrence(&CalendarEvent {
            shared_events: Vec::default(),
            calendar_id: "xxx".into(),
            start_time: 1_745_366_400,
            end_time: 1_745_452_800,
            full_day: true,
            recurrence_id: None,
            address_key_packet: None,
            shared_key_packet: None,
            attendees_events: Vec::default(),
            attendees: Vec::default(),
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
            shared_events: Vec::default(),
            calendar_id: "xxx".into(),
            start_time: 1_528_972_200,
            end_time: 1_528_976_700,
            full_day: false,
            recurrence_id: None,
            address_key_packet: None,
            shared_key_packet: None,
            attendees_events: Vec::default(),
            attendees: Vec::default(),
        })
        .unwrap();

        let expected = RsvpOccurrence::DateTime {
            starts_at: "2018-06-14T10:30:00Z".parse().unwrap(),
            ends_at: "2018-06-14T11:45:00Z".parse().unwrap(),
        };

        assert_eq!(expected, actual);
    }
}
