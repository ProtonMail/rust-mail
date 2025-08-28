use crate::{
    CalendarBootstrapExt, CalendarDecryptorKeys, CalendarEventPayloadExt, RsvpAnswer,
    RsvpAnswerError, RsvpAnswerResult, RsvpAttendee, RsvpCache, RsvpError, RsvpEvent, RsvpKeys,
    RsvpMail, RsvpResult,
};
use itertools::Itertools;
use jiff::Zoned;
use proton_calendar_api::{
    CalendarAttendeeId, CalendarAttendeeStatus, CalendarAttendeeToken, CalendarBootstrap,
    CalendarColor, CalendarEvent, CalendarNotificationsUpdate, ProtonCalendar,
};
use proton_core_api::session::Session;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_calendar::{CalendarKeyPacketUpgrader, KeyPacketRef, LockedCalendarKey};
use proton_ical as ical;
use std::{iter, ops};
use tracing::{debug, error, info, instrument, warn};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run<P, K, M>(
    api: &Session,
    pgp: &P,
    keys: &K,
    cache: &impl RsvpCache,
    sender: M,
    event: &mut RsvpEvent,
    now: &Zoned,
    answer: RsvpAnswer,
) -> RsvpAnswerResult<(), K, M>
where
    P: PGPProviderSync,
    K: RsvpKeys,
    M: RsvpMail,
{
    info!(?now, ?answer, "Answering");

    let Init {
        event,
        calendar,
        keys,
    } = init::<P, K, M>(api, pgp, keys, cache, event).await?;

    let steps = plan(pgp, &keys, &calendar, &event, answer)?;

    exec::<P, K, M>(api, pgp, &keys, sender, calendar, event, now, answer, steps).await?;

    Ok(())
}

#[derive(Clone, Debug)]
enum Step {
    /// See [`exec_upgrade_event()`].
    UpgradeEvent { key_packet: String },

    /// See [`exec_update_attendee()`].
    UpdateAttendee {
        event_idx: Option<usize>,
        att_id: CalendarAttendeeId,
        att_old_status: CalendarAttendeeStatus,
        att_new_status: CalendarAttendeeStatus,
    },

    /// See [`exec_update_event()`].
    UpdateEvent {
        event_idx: Option<usize>,
        event_color: Option<CalendarColor>,
        event_notifs: CalendarNotificationsUpdate,
    },

    /// See [`exec_notify_organizer()`].
    NotifyOrganizer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EventType {
    Parent,
    Child, // aka single edit
}

#[instrument(skip_all)]
async fn init<'a, P, K, M>(
    api: &Session,
    pgp: &P,
    keys: &K,
    cache: &impl RsvpCache,
    event: &'a mut RsvpEvent,
) -> RsvpAnswerResult<Init<'a, P>, K, M>
where
    P: PGPProviderSync,
    K: RsvpKeys,
    M: RsvpMail,
{
    let event = AnswerableRsvpEvent::new(event).ok_or(RsvpError::NonAnswerable)?;

    let calendar = cache
        .get_calendar_bootstrap(&event.raw().calendar_id, || {
            // We need for the returned future to be static, otherwise rustc has
            // hard time proving sendness
            let api = api.clone();
            let id = event.raw().calendar_id.clone();

            async move { api.get_calendar_bootstrap(&id).await }
        })
        .await
        .map_err(RsvpError::from)?;

    let keys = CalendarDecryptorKeys::rsvp(pgp, keys, &calendar, event.raw())
        .await
        .map_err(RsvpAnswerError::Keys)?;

    Ok(Init {
        event,
        calendar,
        keys,
    })
}

#[instrument(skip_all)]
fn plan<P>(
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    calendar: &CalendarBootstrap,
    event: &AnswerableRsvpEvent,
    answer: RsvpAnswer,
) -> RsvpResult<Vec<Step>>
where
    P: PGPProviderSync,
{
    info!("Planning");

    let token = event
        .user_attendee()
        .token
        .as_ref()
        .ok_or(RsvpError::UnknownAttendee)?;

    let events = {
        let parent = iter::once((event.raw(), EventType::Parent, None));

        let children = event
            .children
            .iter()
            .enumerate()
            .map(|(event_idx, event)| (event, EventType::Child, Some(event_idx)));

        parent.chain(children)
    };

    let parent_calendar_id = &event.raw().calendar_id;

    let steps = events
        .filter(|(event, _, _)| event.calendar_id == *parent_calendar_id)
        .map(|(event, event_ty, event_idx)| {
            plan_event(
                pgp, keys, calendar, event, event_ty, event_idx, answer, token,
            )
        })
        .flatten_ok();

    // For Proton-to-Proton invites the most important action is updating the
    // calendar event - sending email is sorta optional in the sense that we do
    // it only for bookkeeping purposes, so that organizer can double-check the
    // invitation went through.
    //
    // This situation is reversed for external invites where sending the reply
    // is the most important bit - if it fails, we don't want to update user's
    // calendar.
    let notify = iter::once(Ok(Step::NotifyOrganizer));

    if event.raw().is_proton_proton_invite {
        steps.chain(notify).collect()
    } else {
        notify.chain(steps).collect()
    }
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip_all, fields(id = event.id.as_str()))]
fn plan_event<P>(
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    calendar: &CalendarBootstrap,
    event: &CalendarEvent,
    event_ty: EventType,
    event_idx: Option<usize>,
    answer: RsvpAnswer,
    token: &CalendarAttendeeToken,
) -> RsvpResult<Vec<Step>>
where
    P: PGPProviderSync,
{
    debug!("Planning event");

    // If this event was already cancelled, don't reply to it.
    //
    // This matters mostly for recurring events with single edits where you can
    // easily end up in a situation where the recurring event itself (as in: the
    // series) is active, but some of its single edits are not.
    //
    // When someone then changes a reply on the series and we update the single
    // edits, we don't want to bother with the cancelled ones.
    if !event.calendar_events.is_empty() {
        let decryptor = calendar.create_decryptor(pgp, keys, event)?;

        for event in &event.calendar_events {
            let event = event.decrypt_and_parse(pgp, &decryptor)?;

            if event
                .status
                .is_some_and(|status| status == ical::Status::Cancelled)
            {
                return Ok(Vec::new());
            }
        }
    }

    // Find out current reply for this event (Unanswered/No/Maybe/Yes).
    //
    // We need this mostly for notification purposes - if user replies for the
    // first time (e.g. Unanswered -> Yes) or changes their answer (Yes -> No),
    // we might have to create, update or delete the notifications (alerts).
    let (att_id, att_old_status) = event
        .attendees
        .iter()
        .find_map(|att| {
            if att.token == *token {
                Some((att.id.clone(), att.status))
            } else {
                None
            }
        })
        .ok_or_else(|| {
            error!("Couldn't find attendee metadata");
            RsvpError::UnknownAttendee
        })?;

    // Usually whatever user has decided (Yes/Maybe/No) is exactly whatever gets
    // saved and sent to the organizer - there's one edge case though, recurring
    // events with single edits.
    //
    // Say, we've got a recurring event "ice bucket challenge" with two single
    // edits (with two "exceptions", so to say):
    //
    // - renamed to "ice bucket challenge with eminem" on 2018-01-01,
    // - renamed to "ice bucket challenge with vsauce" on 2018-01-02.
    //
    // That's three events in total and user can provide a different answer for
    // each of them:
    //
    // - "ice bucket challenge" = maybe,
    // - "ice bucket challenge with eminem" = no,
    // - "ice bucket challenge with vsauce" = yes.
    //
    // Now, the tricky part appears whenever user decides to change the answer
    // on the *parent* event ("ice bucket challenge") - if that happens, we have
    // to iterate through all of the child-events ("... with eminem", "... with
    // vsauce") and reset their answers back to unanswered, unless the answer
    // on the child-event already happens match the new answer on the parent.
    //
    // So following the example above, we'd have the following two scenarios:
    //
    // 1. User changes reply on "ice bucket challenge" to "no" - then:
    //    - we keep the answer on the Eminem's event, since it already is "no",
    //    - we reset the answer on the Vsauce's event.
    //
    // 2. User changes reply on "ice bucket challenge" to "yes" - then:
    //    - we reset the answer on the Eminem's event,
    //    - we keep the answer on the Vsauce's event, since it already is "yes".
    //
    // Answering single edits themselves doesn't trigger this behavior, it's
    // only about answering (or re-answering!) the parent event.
    let att_new_status = {
        let answer = CalendarAttendeeStatus::from(answer);

        match event_ty {
            EventType::Parent => answer,

            EventType::Child => {
                // If the answer already matches what we expect, no need to
                // update the child-event
                if att_old_status == answer {
                    return Ok(Vec::new());
                }

                // Unanswered child-events don't get suddenly answered
                if att_old_status.is_unanswered() {
                    return Ok(Vec::new());
                }

                CalendarAttendeeStatus::Unanswered
            }
        }
    };

    let mut steps = Vec::new();

    // See [`exec_upgrade_event()`] below.
    if let Some(key_packet) = &event.address_key_packet {
        assert_eq!(EventType::Parent, event_ty);

        steps.push(Step::UpgradeEvent {
            key_packet: key_packet.clone(),
        });
    }

    steps.push(Step::UpdateAttendee {
        event_idx,
        att_id,
        att_old_status,
        att_new_status,
    });

    steps.push({
        // We pass-through the same color that event currently has - this makes
        // the color part a no-op update, but it's just that the backend forces
        // us to pass something, we can't simply skip this field or leave it
        // empty.
        let event_color = event.color.clone();

        let event_notifs = plan_event_notifications(
            att_old_status,
            att_new_status,
            event
                .notifications
                .as_ref()
                .is_some_and(|notifs| !notifs.is_empty()),
        );

        Step::UpdateEvent {
            event_idx,
            event_color,
            event_notifs,
        }
    });

    Ok(steps)
}

/// Checks whether we should update the notifications (alerts) for this event.
///
/// When user answers "yes" for the first time, we create the alerts; when user
/// has already answered "yes" in the past and now they change the answer to
/// "no", we delete the alerts etc.
fn plan_event_notifications(
    old_status: CalendarAttendeeStatus,
    new_status: CalendarAttendeeStatus,
    has_notifs: bool,
) -> CalendarNotificationsUpdate {
    // Case 1: `Maybe -> Yes`, `Unanswered -> No` etc.
    //
    // In those transitions we don't update the notifications, because they
    // either are required *and* have been already set up before, or vice versa.
    if old_status.should_notify() == new_status.should_notify() {
        return CalendarNotificationsUpdate::Skip;
    }

    // Case 2: `Unanswered -> Yes`, `No -> Maybe` etc.
    //
    // In those transitions we create the notifications, but only if the event
    // doesn't already have them as we don't want to reset notifications that
    // user has manually created for this event before.
    if new_status.should_notify() {
        return if has_notifs {
            CalendarNotificationsUpdate::Skip
        } else {
            CalendarNotificationsUpdate::SetToDefault
        };
    }

    // Case 3: `Yes -> No`, `Maybe -> Unanswered` etc., where the event has been
    // discarded and the user shouldn't be notified.
    CalendarNotificationsUpdate::SetTo(Vec::new())
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
async fn exec<P, K, M>(
    api: &Session,
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    sender: M,
    calendar: CalendarBootstrap,
    mut event: AnswerableRsvpEvent<'_>,
    now: &Zoned,
    answer: RsvpAnswer,
    steps: Vec<Step>,
) -> RsvpAnswerResult<(), K, M>
where
    P: PGPProviderSync,
    K: RsvpKeys,
    M: RsvpMail,
{
    fn get_event<'a>(
        event: &'a mut AnswerableRsvpEvent,
        idx: Option<usize>,
    ) -> &'a mut CalendarEvent {
        match idx {
            // Unwrap-safety: We generate indices ourselves, they are in bounds
            Some(idx) => &mut event.children[idx],
            None => event.raw_mut(),
        }
    }

    info!("Executing");

    let mut sender = Some(sender);

    for step in steps {
        match step {
            Step::UpgradeEvent { key_packet } => {
                exec_upgrade_event(api, pgp, keys, &mut event, &calendar, key_packet).await?;
            }

            Step::UpdateAttendee {
                event_idx,
                att_id,
                att_old_status,
                att_new_status,
            } => {
                exec_update_attendee(
                    api,
                    now,
                    get_event(&mut event, event_idx),
                    &att_id,
                    att_old_status,
                    att_new_status,
                )
                .await?;

                // Update the [`RsvpEvent`] object so that the updated status is
                // visible on the user interface.
                //
                // We do this only for the parent event (`event_idx = None`),
                // because that's the only event for which we've got the
                // [`RsvpEvent`] object - child events are not displayed to the
                // user and thus don't have their own [`RsvpEvent`]s.
                if event_idx.is_none() {
                    for att in &mut event.attendees {
                        if att.id.as_ref() == Some(&att_id) {
                            att.status = Some(att_new_status);
                        }
                    }
                }
            }

            Step::UpdateEvent {
                event_idx,
                event_color,
                event_notifs,
            } => {
                exec_update_event(
                    api,
                    get_event(&mut event, event_idx),
                    event_color,
                    event_notifs,
                )
                .await?;
            }

            Step::NotifyOrganizer => {
                // Unwrap-safety: `plan()` creates only one step of this kind
                let sender = sender
                    .take()
                    .expect("tried to notify the organizer multiple times");

                exec_notify_organizer::<P, K, M>(
                    api, pgp, keys, sender, &calendar, &event, now, answer,
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// An event is encrypted using random key known as _session key_ which itself
/// is encrypted with either the address key or the calendar key, like:
///
/// ```text
/// let session_key;
///
/// if event.has_address_key_packet:
///     session_key = decrypt(event.address_key_packet, private_address_key)
/// else:
///     session_key = decrypt(event.shared_key_packet, private_calendar_key)
///
/// let stuff = decrypt(event.stuff, session_key)
/// ```
///
/// Most events are encrypted using calendar key, with the only exception being
/// Proton-to-Proton invites - those get encrypted up-front by the organizer
/// using attendee's public key.
///
/// But even though an event can be encrypted using either of the keys, it is
/// more practical for it to be encrypted using the calendar key - this makes
/// sure that user can access their events after they've rotated address keys.
///
/// So when user replies to an event for the first time, we seize this moment to
/// re-encrypt the event using calendar key.
///
/// Note that we don't literally re-encrypt all of the fields, we just switch
/// the session key so that *it* is encrypted using calendar key - the data
/// remains the same, we just change the key representation.
#[instrument(skip_all)]
async fn exec_upgrade_event<P>(
    api: &Session,
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    event: &mut AnswerableRsvpEvent<'_>,
    calendar: &CalendarBootstrap,
    key_packet: String,
) -> RsvpResult<()>
where
    P: PGPProviderSync,
{
    debug!("Upgrading event's encryption");

    let key_packet = {
        let address_keys = keys.event_addr_keys.as_ref().unwrap_or(&keys.cal_addr_keys);

        let calendar_key =
            LockedCalendarKey::from_bootstrap(calendar)?.import(pgp, &keys.cal_addr_keys)?;

        let key_packet = KeyPacketRef::from_base64(&key_packet);

        CalendarKeyPacketUpgrader::upgrade(pgp, address_keys, &calendar_key, key_packet)?
    };

    api.upgrade_calendar_event_invite(
        &event.raw().calendar_id,
        &event.raw().id,
        key_packet.as_base64(),
    )
    .await?;

    // Update the [`CalendarEvent`] object so that if user changes their reply
    // without refreshing the RSVP, our logic is aware of the updated key
    // packets
    event.raw_mut().address_key_packet = None;
    event.raw_mut().shared_key_packet = Some(key_packet.into_base64());

    Ok(())
}

#[instrument(skip_all)]
async fn exec_update_attendee(
    api: &Session,
    now: &Zoned,
    event: &mut CalendarEvent,
    att_id: &CalendarAttendeeId,
    att_old_status: CalendarAttendeeStatus,
    att_new_status: CalendarAttendeeStatus,
) -> RsvpResult<()> {
    debug!(
        ?att_id,
        ?att_old_status,
        ?att_new_status,
        "Updating attendee",
    );

    api.update_calendar_event_attendee_status(
        &event.calendar_id,
        &event.id,
        att_id,
        att_new_status,
        now,
    )
    .await?;

    // Update the [`CalendarEvent`] object so that if user changes their reply
    // without refreshing the RSVP, our logic is aware of the updated attendance
    // statuses
    for att in &mut event.attendees {
        if att.id == *att_id {
            att.status = att_new_status;
        }
    }

    Ok(())
}

#[instrument(skip_all)]
async fn exec_update_event(
    api: &Session,
    event: &CalendarEvent,
    event_color: Option<CalendarColor>,
    event_notifs: CalendarNotificationsUpdate,
) -> RsvpResult<()> {
    debug!(
        cal_id=?event.calendar_id,
        event_id=?event.id,
        ?event_color,
        ?event_notifs,
        "Updating event",
    );

    api.update_calendar_event_personal_part(
        &event.calendar_id,
        &event.id,
        event_color,
        event_notifs,
    )
    .await?;

    Ok(())
}

#[instrument(skip_all)]
#[allow(clippy::needless_lifetimes, reason = "false-positive")]
#[allow(clippy::too_many_arguments)]
async fn exec_notify_organizer<P, K, M>(
    api: &Session,
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    sender: M,
    calendar: &CalendarBootstrap,
    event: &AnswerableRsvpEvent<'_>,
    now: &Zoned,
    answer: RsvpAnswer,
) -> RsvpAnswerResult<(), K, M>
where
    P: PGPProviderSync,
    K: RsvpKeys,
    M: RsvpMail,
{
    debug!("Notifying organizer");

    let body = {
        let email = &event.user_attendee().email;

        let verb = match answer {
            RsvpAnswer::Maybe => "tentatively accepted",
            RsvpAnswer::No => "declined",
            RsvpAnswer::Yes => "accepted",
        };

        let summary = event.summary.as_deref().unwrap_or("(no title)");

        format!("{email} {verb} your invitation to {summary}")
    };

    let ics = build_ics(api, pgp, keys, calendar, event, now, answer).await?;

    sender
        .send(&event.organizer.reply_email, &body, &ics)
        .await
        .map_err(RsvpAnswerError::Mail)?;

    Ok(())
}

/// Builds an `invite.ics` file that's attached to email sent to the organizer.
async fn build_ics<P>(
    api: &Session,
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    calendar: &CalendarBootstrap,
    event: &AnswerableRsvpEvent<'_>,
    now: &Zoned,
    answer: RsvpAnswer,
) -> RsvpResult<String>
where
    P: PGPProviderSync,
{
    debug!("Building *.ics");

    let prodid = ical::utils::prodid();
    let event = build_ics_event(pgp, keys, calendar, event, now, answer)?;
    let timezones = build_ics_timezones(api, &event).await?;

    let cal = ical::VCalendar::new(prodid)
        .with_method(ical::Method::Reply)
        .with_event(event)
        .with_timezones(timezones);

    match cal.validate() {
        ical::ValidatedVCalendar::Clean(cal) => Ok(cal.to_string()),

        // Since we're building the calendar ourselves, getting a validation
        // error here is mildly unexpected - it can happen, though.
        //
        // Notably, if we failed to fetch time zones from the backend, we might
        // generate a reply without any VTIMEZONE object - this is technically
        // illegal, but in practice most clients tend to support such replies
        // anyway, so we might as well just carry on.
        ical::ValidatedVCalendar::Dirty(cal) => {
            for viol in cal.viols() {
                warn!("ics-validator said: {viol}");
            }

            Ok(cal.to_string())
        }
    }
}

fn build_ics_event<P>(
    pgp: &P,
    keys: &CalendarDecryptorKeys<P>,
    calendar: &CalendarBootstrap,
    event: &AnswerableRsvpEvent<'_>,
    now: &Zoned,
    answer: RsvpAnswer,
) -> RsvpResult<ical::VEvent>
where
    P: PGPProviderSync,
{
    let decryptor = calendar.create_decryptor(pgp, keys, event.raw())?;
    let mut lhs = ical::VEvent::default();

    // Event data is split into the clear-text part (like uid and dates) and the
    // encrypted part (like summary and location) - to construct the reply, we
    // need to join info from both of them.
    //
    // Note that we don't merge all of the fields, e.g. we don't care about the
    // alarms.
    for rhs in &event.raw().shared_events {
        let rhs = rhs.decrypt_and_parse(pgp, &decryptor)?;

        lhs.uid = lhs.uid.or(rhs.uid);
        lhs.dtstart = lhs.dtstart.or(rhs.dtstart);
        lhs.dtend = lhs.dtend.or(rhs.dtend);

        lhs.summary = lhs.summary.or(rhs.summary);
        lhs.location = lhs.location.or(rhs.location);
        lhs.organizer = lhs.organizer.or(rhs.organizer);

        lhs.rrule = lhs.rrule.or(rhs.rrule);
        lhs.sequence = lhs.sequence.or(rhs.sequence);
        lhs.recurrence_id = lhs.recurrence_id.or(rhs.recurrence_id);
    }

    let dtstamp = now.in_tz("UTC")?;
    let dtstamp = ical::DateTime::try_from(dtstamp)?;

    let attendee = &event.user_attendee().email;
    let attendee = ical::EmailAddress::from(attendee);
    let attendee = ical::Attendee::from(attendee).with_partstat(answer.into());

    Ok(lhs.with_dtstamp(dtstamp).with_attendee(attendee))
}

async fn build_ics_timezones(
    api: &Session,
    event: &ical::VEvent,
) -> RsvpResult<Vec<ical::VTimeZone>> {
    let dtstart = event.dtstart.as_ref().map(|dtstart| &dtstart.value);
    let dtend = event.dtend.as_ref().map(|dtend| &dtend.value);

    // Step 1: Check which time zones we need (e.g. "Europe/London").
    //
    // Most of the time this will yield just one time zone, but in principle an
    // event can end in a different time zone than the time zone it starts in.
    let timezones: Vec<_> = [dtstart, dtend]
        .into_iter()
        .flatten()
        .filter_map(|date| date.tzid())
        .map(|tzid| tzid.value.as_str())
        .unique()
        .collect();

    // If the event is defined in terms of UTC, we won't have any time zones to
    // fetch - in that case we have to short-circuit, since otherwise the
    // backend will scream at us that we're asking it about zero time zones
    if timezones.is_empty() {
        return Ok(Vec::default());
    }

    // Step 2: Ask backend for mapping from time zone name(s) into object(s).
    //
    // This returns *.ics contents - `BEGIN:VTIMEZONE...` - that describe the
    // time zones, their transitions etc.
    let timezones = api
        .get_calendar_vtimezones(&timezones)
        .await?
        .timezones
        .into_values();

    // Step 3: Parse *.ics contents into time zone objects
    let timezones = timezones
        .into_iter()
        .filter_map(|timezone| match ical::VTimeZone::from_str(&timezone) {
            Ok(timezone) => Some(timezone.tz),

            Err(err) => {
                warn!(?err, ?timezone, "Couldn't parse time zone");

                // While RFC 5545 requires for VTIMEZONEs to be present, most of
                // clients (e.g. Google Calendar) parse time-zone-less invites
                // just fine - so if we weren't able to parse this time zone,
                // let's carry on.
                //
                // Note that this doesn't actually affect the semantics of the
                // invite - time zones are specified within the DTSTART and
                // DTEND properties, and those strings *remain*:
                //
                // ```
                // DTSTART;TZID=Europe/London:20250609T140000
                // DTEND;TZID=Europe/London:20250609T150000
                // ```
                //
                // The only thing that happens in here is that we don't generate
                // the VTIMEZONE object, which - if you think about it - is sort
                // of redundant anyway, because it's duplicated with tzdb data.
                None
            }
        })
        .collect();

    Ok(timezones)
}

/// Wrapper for [`RsvpEvent`] that guarantees we've got access to the underlying
/// [`CalendarEvent`].
///
/// [`CalendarEvent`] is fetched live from Proton Calendar's API - if there's no
/// internet connection, we will be able to generate a valid [`RsvpEvent`] out
/// of the data from `invite.ics`, but the invite itself is not sufficient to be
/// able to _reply_ to it.
///
/// (not to mention that if there's no internet connection, the whole concept of
/// replying doesn't work anyway, because that's an online-only action.)
///
/// This makes the [`CalendarEvent`] field optional within [`RsvpEvent`], which
/// in turn makes having a wrapper like [`Self`] handy for the cases where we
/// know the API calendar event *is* going to be inserted into the invite.
struct AnswerableRsvpEvent<'a>(&'a mut RsvpEvent);

impl<'a> AnswerableRsvpEvent<'a> {
    fn new(event: &'a mut RsvpEvent) -> Option<Self> {
        if event.can_be_answered() {
            Some(Self(event))
        } else {
            None
        }
    }

    #[must_use]
    fn user_attendee(&self) -> &RsvpAttendee {
        // Unwrap-safety: Guarded by constructor
        self.0.user_attendee().unwrap()
    }

    #[must_use]
    fn raw(&self) -> &CalendarEvent {
        // Unwrap-safety: Guarded by constructor
        self.0.raw.as_ref().unwrap()
    }

    #[must_use]
    fn raw_mut(&mut self) -> &mut CalendarEvent {
        // Unwrap-safety: Guarded by constructor
        self.0.raw.as_mut().unwrap()
    }
}

impl ops::Deref for AnswerableRsvpEvent<'_> {
    type Target = RsvpEvent;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl ops::DerefMut for AnswerableRsvpEvent<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

struct Init<'a, P>
where
    P: PGPProviderSync,
{
    event: AnswerableRsvpEvent<'a>,
    calendar: CalendarBootstrap,
    keys: CalendarDecryptorKeys<P>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_event_notifications() {
        // Case 1
        for status in [
            CalendarAttendeeStatus::Unanswered,
            CalendarAttendeeStatus::Maybe,
            CalendarAttendeeStatus::No,
            CalendarAttendeeStatus::Yes,
        ] {
            assert_eq!(
                CalendarNotificationsUpdate::Skip,
                super::plan_event_notifications(status, status, false)
            );
        }

        // Case 2a
        for old_status in [
            CalendarAttendeeStatus::Unanswered,
            CalendarAttendeeStatus::No,
        ] {
            for new_status in [CalendarAttendeeStatus::Maybe, CalendarAttendeeStatus::Yes] {
                assert_eq!(
                    CalendarNotificationsUpdate::Skip,
                    super::plan_event_notifications(old_status, new_status, true)
                );
            }
        }

        // Case 2b
        for old_status in [
            CalendarAttendeeStatus::Unanswered,
            CalendarAttendeeStatus::No,
        ] {
            for new_status in [CalendarAttendeeStatus::Maybe, CalendarAttendeeStatus::Yes] {
                assert_eq!(
                    CalendarNotificationsUpdate::SetToDefault,
                    super::plan_event_notifications(old_status, new_status, false)
                );
            }
        }

        // Case 3
        for old_status in [CalendarAttendeeStatus::Maybe, CalendarAttendeeStatus::Yes] {
            for new_status in [
                CalendarAttendeeStatus::Unanswered,
                CalendarAttendeeStatus::No,
            ] {
                assert_eq!(
                    CalendarNotificationsUpdate::SetTo(Vec::new()),
                    super::plan_event_notifications(old_status, new_status, false)
                );
            }
        }
    }
}
