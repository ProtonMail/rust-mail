mod answer;
mod fetch;

use itertools::Itertools;
use jiff::{
    Zoned,
    civil::{Date, Weekday},
};
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarAttendeeToken, CalendarBootstrap,
    CalendarColor, CalendarEvent, CalendarEventId, CalendarEventRecurrenceId, CalendarEventUid,
    CalendarId,
};
use proton_core_api::{
    service::ApiServiceError,
    services::proton::{AddressId, Proton},
};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::Error as CryptoError;
use proton_ical::{self as ical};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, error::Error, fmt, num::NonZeroU32};
use thiserror::Error;
use tracing::instrument;

#[derive(Clone, PartialEq)]
pub enum RsvpEventId {
    Invite {
        uid: CalendarEventUid,
        rid: Option<CalendarEventRecurrenceId>,
        method: ical::Method,
        invite: Box<ical::VEvent>,
    },
    Reminder {
        cal_id: CalendarId,
        event_id: CalendarEventId,
    },
}

impl RsvpEventId {
    /// Extracts event identifier from `invite.ics` attachment.
    ///
    /// See: [`Self::from_headers()`], [`Self::fetch()`].
    pub fn from_invite(ics: &[u8]) -> RsvpResult<Self> {
        let cal = ical::VCalendar::from_bytes(ics)?.cal;

        let Some(method) = cal.method else {
            return Err(RsvpError::IcsIsNotRsvp);
        };

        let (ical::Method::Cancel | ical::Method::Request) = method else {
            return Err(RsvpError::IcsIsNotRsvp);
        };

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

        Ok(RsvpEventId::Invite {
            uid,
            rid,
            method,
            invite: Box::new(event),
        })
    }

    /// Extracts event identifier from email headers.
    ///
    /// This is required for Proton email remainders which don't generate the
    /// `invite.ics` file.
    ///
    /// See: [`Self::from_invite()`], [`Self::fetch()`].
    #[must_use]
    pub fn from_headers(headers: &HashMap<String, JsonValue>) -> Option<Self> {
        let cid = headers
            .get("X-Pm-Calendar-Calendarid")
            .and_then(|id| id.as_str());

        let eid = headers
            .get("X-Pm-Calendar-Eventid")
            .and_then(|id| id.as_str());

        let intent = headers
            .get("X-Pm-Calendar-Intent")
            .and_then(|intent| intent.as_str());

        if let (Some(cid), Some(eid), Some("reminder")) = (cid, eid, intent) {
            Some(RsvpEventId::Reminder {
                cal_id: cid.into(),
                event_id: eid.into(),
            })
        } else {
            None
        }
    }

    /// Fetches event from the API, decrypts it, and returns its contents.
    ///
    /// Note that this function needs to know the address keys of the currently
    /// logged-in user (i.e. the one who got the invite).
    #[instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch<P, K>(
        &self,
        api: &Proton,
        pgp: &P,
        keys: &K,
        cache: &impl RsvpCache,
        contacts: &impl RsvpContacts,
        now: &Zoned,
        email: &str,
        week_start: Weekday,
    ) -> RsvpFetchResult<Option<RsvpEvent>, K>
    where
        P: PGPProviderSync,
        K: RsvpKeys,
    {
        fetch::run(
            api, pgp, keys, cache, contacts, now, email, week_start, self,
        )
        .await
    }
}

impl fmt::Debug for RsvpEventId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RsvpEventId::Invite { uid, rid, .. } => {
                f.debug_tuple("Invite").field(uid).field(rid).finish()
            }
            RsvpEventId::Reminder { cal_id, event_id } => f
                .debug_tuple("Reminder")
                .field(cal_id)
                .field(event_id)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpEvent {
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub recurrence: Option<RsvpRecurrence>,
    pub occurrence: RsvpOccurrence,
    pub organizer: RsvpOrganizer,
    pub attendees: Vec<RsvpAttendee>,
    pub relation: RsvpRelation,
    pub calendar: Option<RsvpCalendar>,
    pub progress: RsvpProgress,
    pub recency: RsvpRecency,
    pub intent: RsvpIntent,
    pub raw: Option<Box<CalendarEvent>>,
    pub children: Vec<CalendarEvent>,
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
    #[allow(clippy::too_many_arguments)]
    pub async fn answer<P, K, M>(
        &mut self,
        api: &Proton,
        pgp: &P,
        keys: &K,
        cache: &impl RsvpCache,
        sender: M,
        now: &Zoned,
        answer: RsvpAnswer,
    ) -> RsvpAnswerResult<(), K, M>
    where
        P: PGPProviderSync,
        K: RsvpKeys,
        M: RsvpMail,
    {
        answer::run(api, pgp, keys, cache, sender, self, now, answer).await
    }

    #[must_use]
    pub fn user_attendee(&self) -> Option<&RsvpAttendee> {
        if let RsvpRelation::Attendee { attendee_idx } = self.relation {
            Some(&self.attendees[attendee_idx])
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_unanswered(&self) -> bool {
        self.user_attendee()
            .is_some_and(|att| att.status.is_some_and(|stat| stat.is_unanswered()))
    }

    #[must_use]
    pub fn can_be_answered(&self) -> bool {
        self.intent == RsvpIntent::Invite
            && self.recency == RsvpRecency::Fresh
            && self.progress != RsvpProgress::Cancelled
            && self.user_attendee().is_some()
            && self.raw.is_some() // [1]

        // [1] `raw` can be missing only if there's no internet connection - but
        //     if there was no internet connection, we wouldn't be able to
        //     confirm that the invite is fresh
        //
        //     this makes this check overzealous, but it's still nice to have it
        //     as a safeguard as the rsvp answering code requires access to the
        //     raw data
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RsvpRecurrence {
    /// "Every day", "Every second day" etc.
    EveryDay { interval: NonZeroU32 },

    /// "Every Monday", "Every Tuesday or Friday of every other week" etc.
    EveryWeekday {
        interval: NonZeroU32,
        days: Vec<Weekday>,
    },

    /// "Every 14th day of the month", "Every 14th day of every other month"
    /// etc.
    EveryDayOfMonth {
        interval: NonZeroU32,
        days: Vec<NonZeroU32>,
    },

    /// "Every Monday", "Every Friday of every other month" etc.
    EveryWeekdayOfMonth {
        interval: NonZeroU32,
        days: Vec<Weekday>,
    },

    /// "Every first Monday of the month", "Every second Friday of every other
    /// month" etc.
    EveryFixedWeekdayOfMonth {
        interval: NonZeroU32,
        days: Vec<(NonZeroU32, Weekday)>,
    },

    /// "Every last Monday of the month", "Every last Friday of every other
    /// month" etc.
    EveryLastWeekdayOfMonth {
        interval: NonZeroU32,
        days: Vec<Weekday>,
    },

    /// "Every year", "Every other year" etc.
    EveryYear { interval: NonZeroU32 },

    /// Unrecognized.
    Custom(ical::Freq),
}

// TODO (NGC-134) ideally most of this formatting would be shoved into the
//      translation layer, but we don't have it at the moment
impl fmt::Display for RsvpRecurrence {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Joins all items except the last one with comma, as in:
        ///
        /// ```text
        /// Monday, Tuesday and Wednesday
        /// ```
        fn join<T>(mut items: impl ExactSizeIterator<Item = T>) -> String
        where
            T: fmt::Display,
        {
            let len = items.len();
            let lhs = items.by_ref().take(len - 1).join(", ");
            let rhs = items.next().unwrap();

            if lhs.is_empty() {
                rhs.to_string()
            } else {
                // Excuse the lack of oxford, comma.
                format!("{lhs} and {rhs}")
            }
        }

        fn fmt_weekday(day: Weekday) -> &'static str {
            match day {
                Weekday::Monday => "Monday",
                Weekday::Tuesday => "Tuesday",
                Weekday::Wednesday => "Wednesday",
                Weekday::Thursday => "Thursday",
                Weekday::Friday => "Friday",
                Weekday::Saturday => "Saturday",
                Weekday::Sunday => "Sunday",
            }
        }

        fn fmt_weekdays(days: &[Weekday]) -> String {
            join(days.iter().map(|day| fmt_weekday(*day)))
        }

        fn fmt_ordinal(nth: NonZeroU32) -> String {
            let indicator = if (11..=13).contains(&nth.get()) {
                "th"
            } else {
                match nth.get() {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                }
            };

            format!("{nth}{indicator}")
        }

        fn fmt_ordinals(nths: &[NonZeroU32]) -> String {
            join(nths.iter().map(|nth| fmt_ordinal(*nth)))
        }

        fn fmt_fixed_day(nth: NonZeroU32, day: Weekday) -> String {
            let day = fmt_weekday(day);

            match nth.get() {
                1 => format!("first {day}"),
                2 => format!("second {day}"),
                3 => format!("third {day}"),
                4 => format!("fourth {day}"),
                5 => format!("fifth {day}"),

                // Soft-unreachable, but we can't afford to throw an error here
                _ => format!("{} {day}", fmt_ordinal(nth)),
            }
        }

        fn fmt_fixed_days(days: &[(NonZeroU32, Weekday)]) -> String {
            join(days.iter().map(|(nth, day)| fmt_fixed_day(*nth, *day)))
        }

        // ---

        match self {
            RsvpRecurrence::EveryDay { interval } => {
                if interval.get() == 1 {
                    write!(f, "Every day")
                } else {
                    write!(f, "Every {interval} days")
                }
            }

            RsvpRecurrence::EveryWeekday { interval, days } => {
                if interval.get() == 1 {
                    write!(f, "Every {}", fmt_weekdays(days))
                } else {
                    write!(f, "Every {} every {interval} weeks", fmt_weekdays(days))
                }
            }

            RsvpRecurrence::EveryDayOfMonth { interval, days } => {
                if interval.get() == 1 {
                    write!(f, "Every {} day of the month", fmt_ordinals(days))
                } else {
                    write!(
                        f,
                        "Every {} day every {interval} months",
                        fmt_ordinals(days)
                    )
                }
            }

            RsvpRecurrence::EveryWeekdayOfMonth { interval, days } => {
                if interval.get() == 1 {
                    write!(f, "Every {} of the month", fmt_weekdays(days))
                } else {
                    write!(f, "Every {} every {interval} months", fmt_weekdays(days))
                }
            }

            RsvpRecurrence::EveryFixedWeekdayOfMonth { interval, days } => {
                if interval.get() == 1 {
                    write!(f, "Every {} of the month", fmt_fixed_days(days))
                } else {
                    write!(f, "Every {} every {interval} months", fmt_fixed_days(days))
                }
            }

            RsvpRecurrence::EveryLastWeekdayOfMonth { interval, days } => {
                if interval.get() == 1 {
                    write!(f, "Every last {} of the month", fmt_weekdays(days))
                } else {
                    write!(
                        f,
                        "Every last {} every {interval} months",
                        fmt_weekdays(days)
                    )
                }
            }

            RsvpRecurrence::EveryYear { interval } => {
                if interval.get() == 1 {
                    write!(f, "Every year")
                } else {
                    write!(f, "Every {interval} years")
                }
            }

            RsvpRecurrence::Custom(freq) => match freq {
                ical::Freq::Secondly => write!(f, "Custom (secondly)"),
                ical::Freq::Minutely => write!(f, "Custom (minutely)"),
                ical::Freq::Hourly => write!(f, "Custom (hourly)"),
                ical::Freq::Daily => write!(f, "Custom (daily)"),
                ical::Freq::Weekly => write!(f, "Custom (weekly)"),
                ical::Freq::Monthly => write!(f, "Custom (monthly)"),
                ical::Freq::Yearly => write!(f, "Custom (yearly)"),
            },
        }
    }
}

// Note that `ends_at` is inclusive
#[derive(Clone, Debug, PartialEq)]
pub enum RsvpOccurrence {
    Date { starts_at: Date, ends_at: Date },
    DateTime { starts_at: Zoned, ends_at: Zoned },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpOrganizer {
    pub name: Option<String>,

    /// Email address using which we have to reply to the invite.
    ///
    /// This address might be different from `.display_email` for services like
    /// Apple which generate random invite-specific email addresses.
    pub reply_email: String,

    /// Email address using which the organizer presents itself, e.g.
    /// `foo@pm.me`.
    pub display_email: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpAttendee {
    pub id: Option<CalendarAttendeeId>,
    pub token: Option<CalendarAttendeeToken>,
    pub name: Option<String>,
    pub email: String,
    pub status: Option<CalendarAttendeeStatus>,
    pub role: ical::Role,
}

/// Relationship between the user and the invite they are looking at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpRelation {
    /// User is the organizer of this event.
    Organizer,

    /// User is the attendee of this event.
    Attendee { attendee_idx: usize },

    /// User is neither the organizer nor the attendee of this event.
    PartyCrasher,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RsvpCalendar {
    pub id: CalendarId,
    pub name: String,
    pub color: CalendarColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpProgress {
    /// Event has not started yet.
    Pending,

    /// Event is happening right now.
    Ongoing,

    /// Event has ended.
    Ended,

    /// Event has been cancelled.
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpRecency {
    /// Invite is valid and can be replied to.
    Fresh,

    /// Invite is not valid anymore, the underlying event has been updated in
    /// the meantime.
    Outdated,

    /// Invite might be valid or not, there was a problem checking it.
    Unknown(RsvpFetchApiError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpIntent {
    Invite,
    Reminder,
}

impl RsvpIntent {
    #[must_use]
    pub fn is_invite(&self) -> bool {
        matches!(self, Self::Invite)
    }

    #[must_use]
    pub fn is_reminder(&self) -> bool {
        matches!(self, Self::Reminder)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpAnswer {
    Maybe,
    No,  // aka "rejected"
    Yes, // aka "accepted"
}

impl From<RsvpAnswer> for CalendarAttendeeStatus {
    fn from(value: RsvpAnswer) -> Self {
        match value {
            RsvpAnswer::Maybe => CalendarAttendeeStatus::Maybe,
            RsvpAnswer::No => CalendarAttendeeStatus::No,
            RsvpAnswer::Yes => CalendarAttendeeStatus::Yes,
        }
    }
}

impl From<RsvpAnswer> for ical::PartStat {
    fn from(value: RsvpAnswer) -> Self {
        match value {
            RsvpAnswer::Maybe => ical::PartStat::Tentative,
            RsvpAnswer::No => ical::PartStat::Declined,
            RsvpAnswer::Yes => ical::PartStat::Accepted,
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

pub trait RsvpContacts {
    fn get_display_name(&self, email: &str) -> impl Future<Output = Option<String>> + Send;
}

pub trait RsvpKeys {
    type Error: Error;

    fn get_address_keys<P>(
        &self,
        pgp: &P,
        id: &AddressId,
    ) -> impl Future<Output = Result<UnlockedAddressKeys<P>, Self::Error>>
    where
        P: PGPProviderSync;
}

pub trait RsvpMail {
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
    #[error("*.ics is not an RSVP")]
    IcsIsNotRsvp,

    #[error("*.ics contains more than one event")]
    IcsContainsMoreThanOneEvent,

    #[error("*.ics contains no events")]
    IcsContainsNoEvents,

    #[error("*.ics contains an event without uid")]
    IcsEventHasNoUid,

    #[error("*.ics contains an event without dtstart")]
    MissingDtStart,

    #[error("*.ics contains an event with mixed-type dtstart and dtend")]
    MixedDtStartAndDtEnd,

    #[error("Attendee has a non-email address")]
    AttendeeHasNonEmailAddress,

    #[error("Attendee has no X-PM-TOKEN")]
    AttendeeHasNoXPmToken,

    #[error("Attendee is not known")]
    UnknownAttendee,

    #[error("Organizer is not known")]
    UnknownOrganizer(&'static str),

    #[error("Invitation can't be answered")]
    NonAnswerable,

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

impl RsvpError {
    fn is_network_failure(&self) -> bool {
        if let RsvpError::Api(this) = self {
            this.is_network_failure()
        } else {
            false
        }
    }
}

pub type RsvpFetchResult<T, K> = RsvpResult<T, RsvpFetchError<<K as RsvpKeys>::Error>>;

#[derive(Debug, Error)]
pub enum RsvpFetchError<K> {
    Keys(K),
    Rsvp(#[from] RsvpError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RsvpFetchApiError {
    /// Event is not present in Proton Calendar.
    ///
    /// This is just a limitation of the current implementation - it cannot yet
    /// create new events in Proton Calendar, the event must already be there
    /// for user to be able to respond to it.
    EventMissing,

    /// There's no internet connection, Proton Calendar doesn't respond etc.
    NetworkFailure,
}

pub type RsvpAnswerResult<T, K, M> =
    RsvpResult<T, RsvpAnswerError<<K as RsvpKeys>::Error, <M as RsvpMail>::Error>>;

#[derive(Debug, Error)]
pub enum RsvpAnswerError<K, M> {
    Keys(K),
    Mail(M),
    Rsvp(#[from] RsvpError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use pretty_assertions as pa;
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
                DTSTAMP:20180101T120000Z
                SEQUENCE:1
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId::Invite {
            uid: "1234-1234-1234-1234".into(),
            rid: None,
            method: ical::Method::Request,
            invite: Box::new(ical::VEvent {
                dtstamp: Some(ical::DtStamp {
                    value: ical::utils::dt("20180101T120000Z"),
                }),
                sequence: Some(ical::Sequence { value: 1 }),
                ..ical::VEvent::default()
            }),
        };

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn from_invite_cancelled() {
        let actual = RsvpEventId::from_invite(
            indoc! {"
                BEGIN:VCALENDAR
                METHOD:CANCEL
                PRODID:-//Proton AG//iCal//EN
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:1234-1234-1234-1234
                DTSTAMP:20180101T120000Z
                SEQUENCE:1
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId::Invite {
            uid: "1234-1234-1234-1234".into(),
            rid: None,
            method: ical::Method::Cancel,
            invite: Box::new(ical::VEvent {
                dtstamp: Some(ical::DtStamp {
                    value: ical::utils::dt("20180101T120000Z"),
                }),
                sequence: Some(ical::Sequence { value: 1 }),
                ..ical::VEvent::default()
            }),
        };

        pa::assert_eq!(expected, actual);
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
                DTSTAMP:20180101T120000Z
                RECURRENCE-ID:20180101T120000Z
                SEQUENCE:1
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

            RsvpEventId::Invite {
                uid: "1234-1234-1234-1234".into(),
                rid: Some(CalendarEventRecurrenceId::new(rid)),
                method: ical::Method::Request,
                invite: Box::new(ical::VEvent {
                    dtstamp: Some(ical::DtStamp {
                        value: ical::utils::dt("20180101T120000Z"),
                    }),
                    sequence: Some(ical::Sequence { value: 1 }),
                    ..ical::VEvent::default()
                }),
            }
        };

        pa::assert_eq!(expected, actual);
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
                DTSTAMP:20180101T120000Z
                END:VEVENT
                BEGIN:VEVENT
                UID:4321-4321-4321-4321
                DTSTAMP:20180101T130000Z
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap();

        let expected = RsvpEventId::Invite {
            uid: "1234-1234-1234-1234".into(),
            rid: None,
            method: ical::Method::Request,
            invite: Box::new(ical::VEvent {
                dtstamp: Some(ical::DtStamp {
                    value: ical::utils::dt("20180101T120000Z"),
                }),
                sequence: None,
                ..ical::VEvent::default()
            }),
        };

        pa::assert_eq!(expected, actual);
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
                DTSTAMP:20180101T120000Z
                END:VEVENT
                END:VCALENDAR
            "}
            .as_bytes(),
        )
        .unwrap_err();

        assert_eq!(RsvpError::IcsIsNotRsvp.to_string(), actual.to_string());
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
    fn from_headers() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Calendarid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-Pm-Calendar-Eventid", "4321-4321-4321-4321"),
            ("X-Pm-Calendar-Intent", "reminder"),
        ]));

        let expected = Some(RsvpEventId::Reminder {
            cal_id: "1234-1234-1234-1234".into(),
            event_id: "4321-4321-4321-4321".into(),
        });

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn from_headers_invite() {
        let actual = RsvpEventId::from_headers(&headers([
            ("Method", "GET"),
            ("X-Pm-Calendar-Calendarid", "1234-1234-1234-1234"),
            ("FOO", "BAR"),
            ("X-Pm-Calendar-Eventid", "4321-4321-4321-4321"),
            ("X-Pm-Calendar-Intent", "invite"),
        ]));

        // We don't parse invites from headers, because we need to see the
        // `invite.ics` to make sure the email's invite is not out of date.
        pa::assert_eq!(None, actual);
    }

    mod recurrence {
        use super::*;
        use test_case::test_case;

        fn num(nth: u32) -> NonZeroU32 {
            NonZeroU32::new(nth).unwrap()
        }

        struct TestCase {
            given: fn() -> RsvpRecurrence,
            expected: &'static str,
        }

        const TEST_EVERY_DAY_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryDay { interval: num(1) },
            expected: "Every day",
        };

        const TEST_EVERY_DAY_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryDay { interval: num(2) },
            expected: "Every 2 days",
        };

        const TEST_EVERY_WEEKDAY_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryWeekday {
                interval: num(1),
                days: vec![Weekday::Monday, Weekday::Tuesday],
            },
            expected: "Every Monday and Tuesday",
        };

        const TEST_EVERY_WEEKDAY_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryWeekday {
                interval: num(2),
                days: vec![Weekday::Monday, Weekday::Tuesday],
            },
            expected: "Every Monday and Tuesday every 2 weeks",
        };

        const TEST_EVERY_DAY_OF_MONTH_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryDayOfMonth {
                interval: num(1),
                days: vec![num(10), num(20), num(30)],
            },
            expected: "Every 10th, 20th and 30th day of the month",
        };

        const TEST_EVERY_DAY_OF_MONTH_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryDayOfMonth {
                interval: num(2),
                days: vec![num(10), num(20), num(30)],
            },
            expected: "Every 10th, 20th and 30th day every 2 months",
        };

        const TEST_EVERY_WEEKDAY_OF_MONTH_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryWeekdayOfMonth {
                interval: num(1),
                days: vec![Weekday::Friday, Weekday::Saturday, Weekday::Sunday],
            },
            expected: "Every Friday, Saturday and Sunday of the month",
        };

        const TEST_EVERY_WEEKDAY_OF_MONTH_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryWeekdayOfMonth {
                interval: num(2),
                days: vec![Weekday::Friday, Weekday::Saturday, Weekday::Sunday],
            },
            expected: "Every Friday, Saturday and Sunday every 2 months",
        };

        const TEST_EVERY_FIXED_WEEKDAY_OF_MONTH_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryFixedWeekdayOfMonth {
                interval: num(1),
                days: vec![(num(1), Weekday::Friday), (num(2), Weekday::Saturday)],
            },
            expected: "Every first Friday and second Saturday of the month",
        };

        const TEST_EVERY_FIXED_WEEKDAY_OF_MONTH_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryFixedWeekdayOfMonth {
                interval: num(2),
                days: vec![(num(1), Weekday::Friday), (num(2), Weekday::Saturday)],
            },
            expected: "Every first Friday and second Saturday every 2 months",
        };

        const TEST_EVERY_LAST_WEEKDAY_OF_MONTH_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryLastWeekdayOfMonth {
                interval: num(1),
                days: vec![Weekday::Friday],
            },
            expected: "Every last Friday of the month",
        };

        const TEST_EVERY_LAST_WEEKDAY_OF_MONTH_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryLastWeekdayOfMonth {
                interval: num(2),
                days: vec![Weekday::Friday],
            },
            expected: "Every last Friday every 2 months",
        };

        const TEST_EVERY_YEAR_1: TestCase = TestCase {
            given: || RsvpRecurrence::EveryYear { interval: num(1) },
            expected: "Every year",
        };

        const TEST_EVERY_YEAR_2: TestCase = TestCase {
            given: || RsvpRecurrence::EveryYear { interval: num(2) },
            expected: "Every 2 years",
        };

        const TEST_CUSTOM: TestCase = TestCase {
            given: || RsvpRecurrence::Custom(ical::Freq::Secondly),
            expected: "Custom (secondly)",
        };

        #[test_case(TEST_EVERY_DAY_1)]
        #[test_case(TEST_EVERY_DAY_2)]
        #[test_case(TEST_EVERY_WEEKDAY_1)]
        #[test_case(TEST_EVERY_WEEKDAY_2)]
        #[test_case(TEST_EVERY_DAY_OF_MONTH_1)]
        #[test_case(TEST_EVERY_DAY_OF_MONTH_2)]
        #[test_case(TEST_EVERY_WEEKDAY_OF_MONTH_1)]
        #[test_case(TEST_EVERY_WEEKDAY_OF_MONTH_2)]
        #[test_case(TEST_EVERY_FIXED_WEEKDAY_OF_MONTH_1)]
        #[test_case(TEST_EVERY_FIXED_WEEKDAY_OF_MONTH_2)]
        #[test_case(TEST_EVERY_LAST_WEEKDAY_OF_MONTH_1)]
        #[test_case(TEST_EVERY_LAST_WEEKDAY_OF_MONTH_2)]
        #[test_case(TEST_EVERY_YEAR_1)]
        #[test_case(TEST_EVERY_YEAR_2)]
        #[test_case(TEST_CUSTOM)]
        #[allow(clippy::needless_pass_by_value)]
        fn test(case: TestCase) {
            assert_eq!(case.expected, (case.given)().to_string());
        }
    }
}
