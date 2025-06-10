mod fetch;

use chrono::{DateTime, NaiveDate, Utc};
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarAttendeeToken, CalendarColor,
    CalendarEvent, CalendarEventId, CalendarEventRecurrenceId, CalendarId,
};
use proton_core_api::{service::ApiServiceError, services::proton::Proton};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::Error as CryptoError;
use proton_ical::{self as ical, IcsWrite};
use thiserror::Error;
use tracing::instrument;

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpEventId {
    pub uid: CalendarEventId,
    pub recurrence_id: Option<CalendarEventRecurrenceId>,
}

impl RsvpEventId {
    /// Extracts event id from an internal invitation (Proton -> Proton) via an
    /// *.ics file attached to the invitation email.
    ///
    /// See: [`RsvpEventId::fetch()`].
    ///
    /// See also: [`RsvpEventId::from_external()`].
    pub fn from_internal(ics: &[u8]) -> RsvpResult<Self> {
        let cal = ical::VCalendar::from_bytes(ics)?.cal;

        if cal.method != Some(ical::Method::Request) {
            return Err(RsvpError::IcsIsNotRsvpRequest);
        }

        let mut event = cal
            .events
            .into_iter()
            .next()
            .ok_or(RsvpError::IcsContainsNoEvents)?;

        let uid = CalendarEventId::new(
            event
                .uid
                .take()
                .ok_or(RsvpError::IcsEventHasNoUid)?
                .value
                .into_string(),
        );

        let recurrence_id = event
            .recurrence_id
            .take()
            .map(|id| {
                let id = id.value.to_string(ical::Property);

                id.strip_prefix(':').map(ToOwned::to_owned).unwrap_or(id)
            })
            .map(CalendarEventRecurrenceId::new);

        Ok(Self { uid, recurrence_id })
    }

    /// Extracts event id from an external invitation ($vendor -> Proton) via
    /// headers attached to the invitation email.
    ///
    /// See: [`RsvpEventId::fetch()`].
    ///
    /// See also: [`RsvpEventId::from_internal()`].
    pub fn from_external<'a>(
        headers: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> RsvpResult<Self> {
        let mut uid = None;
        let mut recurrence_id = None;

        for (key, val) in headers {
            if key.eq_ignore_ascii_case("x-pm-uid") {
                uid = Some(CalendarEventId::from(val));
            } else if key.eq_ignore_ascii_case("x-pm-recurrenceid") {
                recurrence_id = Some(CalendarEventRecurrenceId::from(val));
            }
        }

        Ok(Self {
            uid: uid.ok_or(RsvpError::MissingXPmUidHeader)?,
            recurrence_id,
        })
    }

    /// Fetches event from the API, decrypts it, and returns its contents.
    ///
    /// Note that this function needs to know the address keys of the currently
    /// logged-in user (i.e. the one who got the invite).
    #[instrument(skip_all)]
    pub async fn fetch<P>(
        &self,
        api: &Proton,
        pgp: &P,
        keys: &UnlockedAddressKeys<P>,
    ) -> RsvpResult<Option<RsvpEvent>>
    where
        P: PGPProviderSync,
    {
        fetch::main(api, pgp, keys, self).await
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpEvent {
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub occurrence: RsvpOccurrence,
    pub attendees: Vec<RsvpAttendee>,
    pub organizer: RsvpOrganizer,
    pub calendar: RsvpCalendar,
    pub raw: Box<CalendarEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RsvpOccurrence {
    /// A full-day event.
    ///
    /// `starts_at` has implied time of 00:00:00, while `ends_at` has 23:59:59,
    /// so a one-day event day will simply say `ends_at == starts_at` etc.
    Date {
        starts_at: NaiveDate,
        ends_at: NaiveDate,
    },

    DateTime {
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpAttendee {
    pub id: CalendarAttendeeId,
    pub token: CalendarAttendeeToken,
    pub email: String,
    pub status: CalendarAttendeeStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpOrganizer {
    pub email: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpCalendar {
    pub id: CalendarId,
    pub name: String,
    pub color: CalendarColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpReply {
    Maybe,
    No,  // aka "rejected"
    Yes, // aka "accepted"
}

pub type RsvpResult<T> = Result<T, RsvpError>;

#[derive(Debug, Error)]
pub enum RsvpError {
    #[error("*.ics is not an RSVP request")]
    IcsIsNotRsvpRequest,

    #[error("*.ics contains more than one event")]
    IcsContainsMoreThanOneEvent,

    #[error("*.ics contains no events")]
    IcsContainsNoEvents,

    #[error("*.ics contains an event without uid")]
    IcsEventHasNoUid,

    #[error("*.ics contains an event without summary")]
    IcsEventHasNoSummary,

    #[error("Missing X-PM-UID header")]
    MissingXPmUidHeader,

    #[error("Couldn't find shared event")]
    CouldntFindSharedEvent,

    #[error("Event's start time is out of range")]
    EventStartTimeIsOutOfRange,

    #[error("Event's end time is out of range")]
    EventEndTimeIsOutOfRange,

    #[error("Attendee has a non-email address")]
    AttendeeHasNonEmailAddress,

    #[error("Attendee has no X-PM-TOKEN")]
    AttendeeHasNoXPmToken,

    #[error("Attendee is not known")]
    AttendeeIsNotKnown,

    #[error("Organizer is not known")]
    OrganizerIsNotKnown,

    #[error("{0}")]
    Api(#[from] ApiServiceError),

    #[error("{0}")]
    Crypto(#[from] CryptoError),

    #[error("{0}")]
    Ical(#[from] ical::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn from_internal() {
        let actual = RsvpEventId::from_internal(
            indoc! {"
                BEGIN:VCALENDAR
                METHOD:REQUEST
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId {
            uid: "1234-1234-1234-1234".into(),
            recurrence_id: None,
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_internal_recurring() {
        let actual = RsvpEventId::from_internal(
            indoc! {"
                BEGIN:VCALENDAR
                METHOD:REQUEST
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000
                RECURRENCE-ID:20180101T120000
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId {
            uid: "1234-1234-1234-1234".into(),
            recurrence_id: Some("20180101T120000".into()),
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_internal_with_multiple_events() {
        let actual = RsvpEventId::from_internal(
            indoc! {"
                BEGIN:VCALENDAR
                METHOD:REQUEST
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000
                END:VEVENT
                BEGIN:VEVENT
                UID:4321-4321-4321-4321
                DTSTAMP:20180101T120000
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId {
            uid: "1234-1234-1234-1234".into(),
            recurrence_id: None,
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_internal_without_method() {
        let actual = RsvpEventId::from_internal(
            indoc! {"
                BEGIN:VCALENDAR
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000
                END:VEVENT
                BEGIN:VEVENT
                UID:4321-4321-4321-4321
                DTSTAMP:20180101T120000
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap_err();

        assert_eq!("*.ics is not an RSVP request", actual.to_string());
    }

    #[test]
    fn from_external() {
        let actual = RsvpEventId::from_external([
            ("Method", "GET"),
            ("X-PM-UID", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
        ])
        .unwrap();

        let expected = RsvpEventId {
            uid: "1234-1234-1234-1234".into(),
            recurrence_id: None,
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_external_recurring() {
        let actual = RsvpEventId::from_external([
            ("Method", "GET"),
            ("X-PM-UID", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-PM-RecurrenceID", "20180101T120000"),
        ])
        .unwrap();

        let expected = RsvpEventId {
            uid: "1234-1234-1234-1234".into(),
            recurrence_id: Some("20180101T120000".into()),
        };

        assert_eq!(expected, actual);
    }
}
