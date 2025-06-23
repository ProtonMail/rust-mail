mod answer;
mod fetch;

use chrono::{DateTime, NaiveDate, Utc};
use jiff::Zoned;
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarAttendeeToken, CalendarBootstrap,
    CalendarColor, CalendarEvent, CalendarEventId, CalendarEventRecurrenceId, CalendarEventUid,
    CalendarId,
};
use proton_core_api::{service::ApiServiceError, services::proton::Proton};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::Error as CryptoError;
use proton_ical::{self as ical};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, error::Error};
use thiserror::Error;
use tracing::instrument;

#[derive(Clone, Debug, PartialEq)]
pub enum RsvpEventId {
    /// Event with up-front known Proton id.
    ///
    /// This is the ideal case, because we can ask the backend about this event
    /// directly, without having to do (somewhat expensive) look-ups.
    ///
    /// In practice we get this only/mostly for email reminders - most RSVPs get
    /// resolved through the (uid,rid) tuple below.
    ///
    /// See: [`Self::from_headers()`].
    Direct(CalendarId, CalendarEventId, RsvpEventType),

    /// Event for which we know only the uid and possibly recurrence id.
    ///
    /// This is a less ideal case, because we have to remap this (uid,rid) tuple
    /// into a Proton event id when we're fetching the event.
    ///
    /// See: [`Self::from_invite()`].
    Indirect(CalendarEventUid, Option<CalendarEventRecurrenceId>),
}

impl RsvpEventId {
    #[doc(hidden)]
    #[must_use]
    pub fn direct(cid: &str, eid: &str, ty: RsvpEventType) -> Self {
        RsvpEventId::Direct(cid.into(), eid.into(), ty)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn indirect(uid: &str, rid: Option<i64>) -> Self {
        let uid = uid.into();
        let rid = rid.map(CalendarEventRecurrenceId::new);

        RsvpEventId::Indirect(uid, rid)
    }

    /// Extracts event identifier from `invite.ics` attachment.
    ///
    /// See: [`Self::from_headers()`], [`Self::fetch()`].
    pub fn from_invite(ics: &[u8]) -> RsvpResult<Self> {
        let cal = ical::VCalendar::from_bytes(ics)?.cal;

        if cal.method != Some(ical::Method::Request) {
            return Err(RsvpError::IcsIsNotRsvpRequest);
        }

        let mut event = cal
            .events
            .into_iter()
            .next()
            .ok_or(RsvpError::IcsContainsNoEvents)?;

        let uid = CalendarEventUid::new(
            event
                .uid
                .take()
                .ok_or(RsvpError::IcsEventHasNoUid)?
                .value
                .into_string(),
        );

        let rid = event
            .recurrence_id
            .take()
            .map(|rid| {
                Zoned::try_from(rid.value)
                    .map(|rid| rid.timestamp().as_second())
                    .map(CalendarEventRecurrenceId::new)
            })
            .transpose()?;

        Ok(RsvpEventId::Indirect(uid, rid))
    }

    /// Extracts event identifier from email headers.
    ///
    /// This comes handy mostly for Proton email remainders which don't generate
    /// the `invite.ics` file, but just provide the event id and recurrence id
    /// via headers.
    ///
    /// See: [`Self::from_invite()`], [`Self::fetch()`].
    pub fn from_headers(headers: &HashMap<String, JsonValue>) -> Option<Self> {
        let cid = headers
            .get("X-Pm-Calendar-Calendarid")
            .and_then(|id| id.as_str());

        let eid = headers
            .get("X-Pm-Calendar-Eventid")
            .and_then(|id| id.as_str());

        let ty = headers
            .get("X-Pm-Calendar-Intent")
            .and_then(|ty| ty.as_str())
            .map_or(RsvpEventType::Invite, |ty| {
                if ty == "reminder" {
                    RsvpEventType::Reminder
                } else {
                    RsvpEventType::Invite
                }
            });

        if let (Some(cid), Some(eid)) = (cid, eid) {
            return Some(RsvpEventId::Direct(cid.into(), eid.into(), ty));
        }

        let uid = headers
            .get("X-Pm-Calendar-Eventuid")?
            .as_str()
            .map(CalendarEventUid::from)?;

        let rid = headers
            .get("X-Pm-Calendar-Occurrence")
            .and_then(|rid| rid.as_str())
            .and_then(|rid| rid.parse().ok())
            .map(CalendarEventRecurrenceId::new);

        Some(RsvpEventId::Indirect(uid, rid))
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
        cache: &impl RsvpCache,
    ) -> RsvpResult<Option<RsvpEvent>>
    where
        P: PGPProviderSync,
    {
        fetch::exec(api, pgp, keys, cache, self).await
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpEvent {
    pub ty: RsvpEventType,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub occurrence: RsvpOccurrence,
    pub attendees: Vec<RsvpAttendee>,
    pub organizer: RsvpOrganizer,
    pub calendar: RsvpCalendar,
    pub status: RsvpStatus,
    pub raw: Box<CalendarEvent>,
}

impl RsvpEvent {
    /// Answers this event.
    ///
    /// This sends an email to the organizer, updates event in the calendar, and
    /// updates `self` to reflect the changes; this function can be called
    /// multiple times to change the answer.
    ///
    /// Note that this function needs to know the address keys of the currently
    /// logged-in user (i.e. the one who got the invite).
    #[instrument(skip_all)]
    pub async fn answer<P, M>(
        &mut self,
        api: &Proton,
        pgp: &P,
        keys: &UnlockedAddressKeys<P>,
        cache: &impl RsvpCache,
        sender: M,
        answer: RsvpAnswer<'_>,
    ) -> RsvpAnswerResult<(), M>
    where
        P: PGPProviderSync,
        M: RsvpMailSender,
    {
        answer::exec(api, pgp, keys, cache, sender, self, answer).await
    }

    #[must_use]
    pub(crate) fn has_notifications(&self) -> bool {
        self.raw
            .notifications
            .as_ref()
            .is_some_and(|n| !n.is_empty())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpEventType {
    Invite,
    Reminder,
}

impl RsvpEventType {
    #[must_use]
    pub fn is_invite(&self) -> bool {
        matches!(self, Self::Invite)
    }

    #[must_use]
    pub fn is_reminder(&self) -> bool {
        matches!(self, Self::Reminder)
    }
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
pub enum RsvpStatus {
    Active,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpAnswer<'a> {
    pub now: Zoned,
    pub email: &'a str,
    pub status: RsvpAnswerStatus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpAnswerStatus {
    Maybe,
    No,  // aka "rejected"
    Yes, // aka "accepted"
}

impl From<RsvpAnswerStatus> for CalendarAttendeeStatus {
    fn from(value: RsvpAnswerStatus) -> Self {
        match value {
            RsvpAnswerStatus::Maybe => CalendarAttendeeStatus::Maybe,
            RsvpAnswerStatus::No => CalendarAttendeeStatus::No,
            RsvpAnswerStatus::Yes => CalendarAttendeeStatus::Yes,
        }
    }
}

impl From<RsvpAnswerStatus> for ical::PartStat {
    fn from(value: RsvpAnswerStatus) -> Self {
        match value {
            RsvpAnswerStatus::Maybe => ical::PartStat::Tentative,
            RsvpAnswerStatus::No => ical::PartStat::Declined,
            RsvpAnswerStatus::Yes => ical::PartStat::Accepted,
        }
    }
}

pub trait RsvpCache {
    fn get_calendar_bootstrap<E, Fn, Fut>(
        &self,
        id: &CalendarId,
        fetch: Fn,
    ) -> impl Future<Output = Result<CalendarBootstrap, E>> + Send
    where
        Fn: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<CalendarBootstrap, E>> + Send;
}

pub trait RsvpMailSender {
    type Error: Error;

    /// Sends an email response to the organizer.
    ///
    /// - `to` is the organizer's address,
    /// - `body` is the message, unencrypted ("xxx accepted your invitation to yyy"),
    /// - `ics` is the `invite.ics` attachment, unencrypted.
    ///
    /// This action corresponds to:
    ///
    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Message/operation/post_mail-v4-messages-send-direct>
    ///
    /// ... but we go through a trait to avoid pulling mail logic directly into
    /// this crate (circular dependency).
    fn send(
        self,
        to: &str,
        body: &str,
        ics: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub type RsvpResult<T, E = RsvpError> = Result<T, E>;

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

    #[error("Missing X-PM-UID header")]
    MissingXPmUidHeader,

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

    #[error("Event is a reminder, not an invite")]
    EventIsReminder,

    #[error("{0}")]
    Api(#[from] ApiServiceError),

    #[error("{0}")]
    Crypto(#[from] CryptoError),

    #[error("{0}")]
    Ical(#[from] ical::Error),

    #[error("{0}")]
    IcalDateTime(#[from] ical::DateTimeError),

    #[error("{0}")]
    Jiff(#[from] jiff::Error),
}

pub type RsvpAnswerResult<T, M> = RsvpResult<T, RsvpAnswerError<<M as RsvpMailSender>::Error>>;

#[derive(Debug, Error)]
pub enum RsvpAnswerError<E> {
    Rsvp(#[from] RsvpError),
    Mail(E),
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use std::str::FromStr;

    #[test]
    fn from_invite() {
        let actual = RsvpEventId::from_invite(
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

        let expected = RsvpEventId::Indirect("1234-1234-1234-1234".into(), None);

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_invite_recurring() {
        let actual = RsvpEventId::from_invite(
            indoc! {"
                BEGIN:VCALENDAR
                METHOD:REQUEST
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000
                RECURRENCE-ID:20180101T120000Z
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = {
            let rid = Zoned::from_str("20180101T120000[UTC]")
                .unwrap()
                .timestamp()
                .as_second();

            RsvpEventId::Indirect(
                "1234-1234-1234-1234".into(),
                Some(CalendarEventRecurrenceId::new(rid)),
            )
        };

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_invite_with_multiple_events() {
        let actual = RsvpEventId::from_invite(
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

        let expected = RsvpEventId::Indirect("1234-1234-1234-1234".into(), None);

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_invite_without_method() {
        let actual = RsvpEventId::from_invite(
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

    fn headers<'a>(kv: impl IntoIterator<Item = (&'a str, &'a str)>) -> HashMap<String, JsonValue> {
        kv.into_iter()
            .map(|(key, value)| {
                let key = key.to_string();
                let value = JsonValue::String(value.to_string());

                (key, value)
            })
            .collect()
    }

    #[test]
    fn from_headers_direct() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Calendarid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-Pm-Calendar-Eventid", "4321-4321-4321-4321"),
        ]));

        let expected = Some(RsvpEventId::Direct(
            "1234-1234-1234-1234".into(),
            "4321-4321-4321-4321".into(),
            RsvpEventType::Invite,
        ));

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_headers_direct_reminder() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Calendarid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-Pm-Calendar-Eventid", "4321-4321-4321-4321"),
            ("X-Pm-Calendar-Intent", "reminder"),
        ]));

        let expected = Some(RsvpEventId::Direct(
            "1234-1234-1234-1234".into(),
            "4321-4321-4321-4321".into(),
            RsvpEventType::Reminder,
        ));

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_headers_indirect() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Eventuid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
        ]));

        let expected = Some(RsvpEventId::Indirect("1234-1234-1234-1234".into(), None));

        assert_eq!(expected, actual);
    }

    #[test]
    fn from_headers_indirect_recurring() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Eventuid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-Pm-Calendar-Occurrence", "1514804400"),
        ]));

        let expected = Some(RsvpEventId::Indirect(
            "1234-1234-1234-1234".into(),
            Some(CalendarEventRecurrenceId::new(1_514_804_400)),
        ));

        assert_eq!(expected, actual);
    }
}
