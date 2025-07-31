use super::state::MailUserContextPtr;
use crate::core::datatypes::UnixTimestamp;
use crate::errors::{ProtonError, RsvpEventGetResult, VoidAnswerRsvpResult};
use crate::uniffi_async;
use itertools::Itertools;
use parking_lot::Mutex;
use proton_calendar_api as cal_api;
use proton_calendar_common as cal;
use proton_core_common::datatypes::UnixTimestamp as RealUnixTimestamp;
use proton_ical as ical;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_mail_common::{self as mail, MailUserContext};
use std::sync::Arc;
use tracing::{error, warn};
use uniffi::{Enum, Object, Record};

#[derive(Object)]
pub struct RsvpEventServiceProvider {
    ctx: MailUserContextPtr,
    rsvp: mail::RsvpEventId,
}

impl RsvpEventServiceProvider {
    #[must_use]
    pub fn new(ctx: MailUserContextPtr, rsvp: mail::RsvpEventId) -> Self {
        Self { ctx, rsvp }
    }

    fn ctx(&self) -> Result<Arc<MailUserContext>, RealProtonMailError> {
        self.ctx
            .upgrade()
            .ok_or(RealProtonMailError::Unexpected(Unexpected::Internal))
    }
}

#[uniffi_export]
impl RsvpEventServiceProvider {
    /// Fetches event details from the API.
    ///
    /// Note that this might return `None` - this will be the case e.g. for
    /// reminders when there's no network connection available (since we need
    /// network connection in order to fetch reminder details from the API).
    pub async fn event_service(self: Arc<Self>) -> Option<Arc<RsvpEventService>> {
        uniffi_async(async move {
            let ctx = self.ctx()?;
            let mut tether = ctx.user_stash().connection();
            let rsvp = self.rsvp.fetch(&ctx, &mut tether).await?;

            if let Some(rsvp) = rsvp {
                Ok(Some(Arc::new(RsvpEventService::new(
                    self.ctx.clone(),
                    rsvp,
                ))))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|err: RealProtonMailError| warn!(?err, "Couldn't fetch RSVP"))
        .ok()
        .flatten()
    }
}

#[derive(Object)]
pub struct RsvpEventService {
    ctx: MailUserContextPtr,
    rsvp: Mutex<mail::RsvpEvent>,
}

impl RsvpEventService {
    #[must_use]
    fn new(ctx: MailUserContextPtr, rsvp: mail::RsvpEvent) -> Self {
        Self {
            ctx,
            rsvp: Mutex::new(rsvp),
        }
    }

    fn ctx(&self) -> Result<Arc<MailUserContext>, RealProtonMailError> {
        self.ctx
            .upgrade()
            .ok_or(RealProtonMailError::Unexpected(Unexpected::Internal))
    }
}

#[uniffi_export]
impl RsvpEventService {
    /// Answers this event.
    ///
    /// After returned `Future` succeeds, call [`Self::details()`] to get an
    /// updated event object (with refreshed attendee status).
    #[returns(VoidAnswerRsvpResult)]
    pub async fn answer(self: Arc<Self>, answer: RsvpAnswer) -> Result<(), ProtonError> {
        uniffi_async(async move {
            let ctx = self.ctx()?;
            let mut tether = ctx.user_stash().connection();
            let mut rsvp = self.rsvp.lock().clone();

            rsvp.answer(&ctx, &mut tether, answer.into()).await?;

            *self.rsvp.lock() = rsvp;

            Ok(())
        })
        .await
        .map_err(RealProtonMailError::into)
        .into()
    }

    #[returns(RsvpEventGetResult)]
    pub fn get(&self) -> Result<RsvpEvent, RealProtonMailError> {
        (&*self.rsvp.lock()).try_into()
    }
}

#[derive(Clone, Debug, Record)]
pub struct RsvpEvent {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub recurrence: Option<String>,
    pub starts_at: UnixTimestamp,
    pub ends_at: UnixTimestamp,
    pub occurrence: RsvpOccurrence,
    pub organizer: RsvpOrganizer,
    pub attendees: Vec<RsvpAttendee>,
    pub user_attendee_idx: Option<u32>,
    pub calendar: Option<RsvpCalendar>,
    pub state: RsvpState,
}

impl TryFrom<&mail::RsvpEvent> for RsvpEvent {
    type Error = RealProtonMailError;

    fn try_from(event: &mail::RsvpEvent) -> Result<Self, RealProtonMailError> {
        let user_attendee_idx = event.user_attendee_idx.map(|idx| {
            // Unwrap-safety: 4 million attendees would make for quite a big
            // party (uniffi doesn't support `usize`).
            u32::try_from(idx).unwrap()
        });

        let (starts_at, ends_at, occurrence) = match &event.occurrence {
            cal::RsvpOccurrence::Date { starts_at, ends_at } => {
                // [1] those operations can fail if the submitted date is funky
                //     (think 9999-12-31) - and while it's almost impossible to
                //     get to this point with such a date, the chances are
                //     greater than zero considering that we load stuff directly
                //     from a "third party" `invite.ics` attachment

                let starts_at = starts_at
                    .in_tz("UTC")
                    .map_err(|err| {
                        error!(?err, "in_tz() failed for starts_at"); // [1]
                        RealProtonMailError::Unexpected(Unexpected::Unknown)
                    })?
                    .start_of_day()
                    .map_err(|err| {
                        error!(?err, "start_of_day() failed for starts_at"); // [1]
                        RealProtonMailError::Unexpected(Unexpected::Unknown)
                    })?;

                let ends_at = ends_at
                    .in_tz("UTC")
                    .map_err(|err| {
                        error!(?err, "in_tz() failed for ends_at"); // [1]
                        RealProtonMailError::Unexpected(Unexpected::Unknown)
                    })?
                    .end_of_day()
                    .map_err(|err| {
                        error!(?err, "end_of_day() failed for ends_at"); // [1]
                        RealProtonMailError::Unexpected(Unexpected::Unknown)
                    })?;

                (starts_at, ends_at, RsvpOccurrence::Date)
            }

            cal::RsvpOccurrence::DateTime { starts_at, ends_at } => {
                (starts_at.clone(), ends_at.clone(), RsvpOccurrence::DateTime)
            }
        };

        Ok(Self {
            id: event.raw.as_ref().map(|event| event.id.to_string()),
            summary: event.summary.clone(),
            location: event.location.clone(),
            description: event.description.clone(),
            recurrence: event.recurrence.as_ref().map(ToString::to_string),
            starts_at: RealUnixTimestamp::from(&starts_at).into(),
            ends_at: RealUnixTimestamp::from(&ends_at).into(),
            occurrence,
            organizer: (&event.organizer).into(),
            attendees: event.attendees.iter().map_into().collect(),
            user_attendee_idx,
            calendar: event.calendar.as_ref().map(Into::into),
            state: event.into(),
        })
    }
}

#[derive(Clone, Debug, Enum)]
pub enum RsvpOccurrence {
    /// Full-day event.
    Date,

    /// Part-day event.
    DateTime,
}

#[derive(Clone, Debug, Record)]
pub struct RsvpOrganizer {
    pub name: Option<String>,
    pub email: String,
}

impl From<&cal::RsvpOrganizer> for RsvpOrganizer {
    fn from(org: &cal::RsvpOrganizer) -> Self {
        Self {
            name: org.name.clone(),
            email: org.email.clone(),
        }
    }
}

#[derive(Clone, Debug, Record)]
pub struct RsvpAttendee {
    pub name: Option<String>,
    pub email: String,
    pub status: RsvpAttendeeStatus,
}

impl From<&cal::RsvpAttendee> for RsvpAttendee {
    fn from(att: &cal::RsvpAttendee) -> Self {
        let status = att
            .status
            .map_or(RsvpAttendeeStatus::Unanswered, Into::into);

        Self {
            name: att.name.clone(),
            email: att.email.clone(),
            status,
        }
    }
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpAttendeeStatus {
    Unanswered,
    Maybe,
    No,
    Yes,
}

impl From<cal_api::CalendarAttendeeStatus> for RsvpAttendeeStatus {
    fn from(status: cal_api::CalendarAttendeeStatus) -> Self {
        match status {
            cal_api::CalendarAttendeeStatus::Unanswered => Self::Unanswered,
            cal_api::CalendarAttendeeStatus::Maybe => Self::Maybe,
            cal_api::CalendarAttendeeStatus::No => Self::No,
            cal_api::CalendarAttendeeStatus::Yes => Self::Yes,
        }
    }
}

#[derive(Clone, Debug, Record)]
pub struct RsvpCalendar {
    pub id: String,
    pub name: String,

    /// Calendar's color, as a CSS hex-string (e.g. `#aabbcc`)
    pub color: String,
}

impl From<&cal::RsvpCalendar> for RsvpCalendar {
    fn from(cal: &cal::RsvpCalendar) -> Self {
        Self {
            id: cal.id.to_string(),
            name: cal.name.clone(),
            color: cal.color.get().to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpState {
    /// RSVP is an invite that can be answered.
    AnswerableInvite {
        progress: RsvpProgress,
        attendance: RsvpAttendance,
    },

    /// RSVP is an invite that cannot be answered anymore.
    UnanswerableInvite { reason: RsvpUnanswerableReason },

    /// RSVP is an invite for a now-cancelled event.
    CancelledInvite { is_outdated: bool },

    /// RSVP is a reminder.
    Reminder { progress: RsvpProgress },

    /// RSVP is a reminder for a now-cancelled event.
    ///
    /// (the terminology is mildly off here, it's not the reminder that got
    /// cancelled - it's the reminder's event.)
    CancelledReminder,
}

impl From<&mail::RsvpEvent> for RsvpState {
    fn from(event: &mail::RsvpEvent) -> Self {
        let attendance = match event.user_attendee().map(|att| att.role) {
            Some(ical::Role::ReqParticipant) => RsvpAttendance::Required,
            _ => RsvpAttendance::Optional,
        };

        let progress = match event.progress {
            cal::RsvpProgress::Pending => Some(RsvpProgress::Pending),
            cal::RsvpProgress::Ongoing => Some(RsvpProgress::Ongoing),
            cal::RsvpProgress::Ended => Some(RsvpProgress::Ended),
            cal::RsvpProgress::Cancelled => None,
        };

        if !event.is_address_correct() {
            return RsvpState::UnanswerableInvite {
                reason: RsvpUnanswerableReason::AddressIsIncorrect,
            };
        }

        if event.user_attendee_idx.is_none() {
            return RsvpState::UnanswerableInvite {
                reason: RsvpUnanswerableReason::UserIsOrganizer,
            };
        }

        match (event.intent, event.recency, progress) {
            (cal::RsvpIntent::Invite, cal::RsvpRecency::Fresh, Some(progress)) => {
                RsvpState::AnswerableInvite {
                    progress,
                    attendance,
                }
            }

            (cal::RsvpIntent::Invite, cal::RsvpRecency::Outdated, Some(_)) => {
                RsvpState::UnanswerableInvite {
                    reason: RsvpUnanswerableReason::InviteIsOutdated,
                }
            }

            (cal::RsvpIntent::Invite, cal::RsvpRecency::Unknown, Some(_)) => {
                RsvpState::UnanswerableInvite {
                    reason: RsvpUnanswerableReason::InviteHasUnknownRecency,
                }
            }

            (cal::RsvpIntent::Invite, recency, None) => RsvpState::CancelledInvite {
                is_outdated: recency == cal::RsvpRecency::Outdated,
            },

            (cal::RsvpIntent::Reminder, _, Some(progress)) => RsvpState::Reminder { progress },
            (cal::RsvpIntent::Reminder, _, None) => RsvpState::CancelledReminder,
        }
    }
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpProgress {
    /// Event has not started yet.
    Pending,

    /// Event is happening right now.
    Ongoing,

    /// Event has ended.
    Ended,
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpAttendance {
    /// User might reply to this invitation.
    Optional,

    /// User must reply to this invitation.
    Required,
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpUnanswerableReason {
    /// User is the organizer of this event.
    UserIsOrganizer,

    /// User is looking at a stale `invite.ics`.
    InviteIsOutdated,

    /// We couldn't confirm whether the invite is stale or fresh; there's
    /// probably no network connection.
    InviteHasUnknownRecency,

    /// User's address is either disabled or otherwise cannot be used to send
    /// the reply.
    AddressIsIncorrect,
}

#[derive(Clone, Copy, Debug, Enum)]
pub enum RsvpAnswer {
    Maybe,
    No,
    Yes,
}

impl From<RsvpAnswer> for cal::RsvpAnswer {
    fn from(answer: RsvpAnswer) -> Self {
        match answer {
            RsvpAnswer::Maybe => Self::Maybe,
            RsvpAnswer::No => Self::No,
            RsvpAnswer::Yes => Self::Yes,
        }
    }
}
