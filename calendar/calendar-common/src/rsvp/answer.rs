use super::{
    RsvpAnswer, RsvpAnswerResult, RsvpAnswerStatus, RsvpCache, RsvpError, RsvpEvent,
    RsvpMailSender, RsvpResult,
};
use crate::{CalendarBootstrapExt, CalendarEventPayloadExt, RsvpAnswerError};
use itertools::Itertools;
use proton_calendar_api::{
    CalendarAttendeeStatus, CalendarBootstrap, CalendarNotificationsUpdate, ProtonCalendar,
};
use proton_core_api::services::proton::Proton;
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_calendar::{
    CalendarEventDecryptor, CalendarKeyPacketUpgrader, KeyPacketRef, LockedCalendarKey,
};
use proton_ical as ical;
use tracing::{info, instrument, warn};

pub(super) async fn exec<P, M>(
    api: &Proton,
    pgp: &P,
    keys: &UnlockedAddressKeys<P>,
    cache: &impl RsvpCache,
    sender: M,
    event: &mut RsvpEvent,
    answer: RsvpAnswer<'_>,
) -> RsvpAnswerResult<(), M>
where
    P: PGPProviderSync,
    M: RsvpMailSender,
{
    info!(?answer, "Answering");

    if event.ty.is_reminder() {
        return Err(RsvpError::EventIsReminder.into());
    }

    let calendar = cache
        .get_calendar_bootstrap(&event.raw.calendar_id, || {
            // We need for the returned future to be static, otherwise rustc has
            // hard time proving sendness
            let api = api.clone();
            let id = event.raw.calendar_id.clone();

            async move { api.get_calendar_bootstrap(&id).await }
        })
        .await
        .map_err(RsvpError::from)?;

    let decryptor = calendar.create_decryptor(pgp, keys, &event.raw)?;

    // For Proton-to-Proton invites the most important action is updating the
    // calendar event - sending email is sorta optional in the sense that we do
    // it only for bookkeeping purposes, so that organizer can double-check the
    // invitation went through.
    //
    // This situation is reversed for external invites where sending the reply
    // is the most important bit - if it fails, we don't want to update user's
    // calendar.
    if event.raw.is_proton_proton_invite {
        update(api, pgp, keys, event, &answer, &calendar).await?;
        notify(api, pgp, sender, event, &answer, &decryptor).await?;
    } else {
        notify(api, pgp, sender, event, &answer, &decryptor).await?;
        update(api, pgp, keys, event, &answer, &calendar).await?;
    }

    Ok(())
}

/// Updates event in the calendar.
#[instrument(skip_all)]
async fn update<P>(
    api: &Proton,
    pgp: &P,
    keys: &UnlockedAddressKeys<P>,
    event: &mut RsvpEvent,
    answer: &RsvpAnswer<'_>,
    calendar: &CalendarBootstrap,
) -> RsvpResult<()>
where
    P: PGPProviderSync,
{
    let has_notifs = event.has_notifications();

    let (att_id, old_status) = event
        .attendees
        .iter_mut()
        .find(|att| att.email == answer.email)
        .map(|att| (&att.id, &mut att.status))
        .ok_or(RsvpError::AttendeeIsNotKnown)?;

    let new_status = answer.status.into();
    let notifs = prepare_notifs(*old_status, new_status, has_notifs);

    // We passthrough the color the event already has - this a no-op update, but
    // backend requires we pass *something*
    let color = event.raw.color.clone();

    // An event is encrypted using random key known as session key which itself
    // is encrypted with either the address key or the calendar key, like:
    //
    //     let session_key;
    //
    //     if event.has_address_key_packet:
    //         session_key = decrypt(event.address_key_packet, private_address_key)
    //     else:
    //         session_key = decrypt(event.shared_key_packet, private_calendar_key)
    //
    //     let stuff = decrypt(event.stuff, session_key)
    //
    // Most events are encrypted using calendar key, with the only exception
    // being Proton-to-Proton invites - there the event organizer encrypts the
    // invitation *up-front* using attendee's public key.
    //
    // But even though an event can be encrypted using either of the keys, it's
    // actually more practical for it to be encrypted using the calendar key -
    // this simplifies calendar logic and makes sure that user can access their
    // events even if they rotate address keys.
    //
    // So when user replies to an event for the first time, we're seizing this
    // moment to re-encrypt the event using calendar key.
    //
    // Note that we don't literally re-encrypt all of the fields, we just switch
    // the session key so that *it* is encrypted using calendar key - the data
    // remains the same, we just change how the key is represented.
    if let Some(key_packet) = &event.raw.address_key_packet {
        info!("Upgrading event to be encrypted with calendar key");

        let key_packet = {
            let calendar_key = LockedCalendarKey::from_bootstrap(calendar)?.import(pgp, keys)?;
            let key_packet = KeyPacketRef::from_base64(key_packet);

            CalendarKeyPacketUpgrader::upgrade(pgp, keys, &calendar_key, key_packet)?
        };

        api.upgrade_calendar_event_invite(
            &event.calendar.id,
            &event.raw.id,
            key_packet.as_base64(),
        )
        .await?;

        // Modify the object as well, in case user re-replies without refreshing
        // the view
        event.raw.address_key_packet = None;
        event.raw.shared_key_packet = Some(key_packet.into_base64());
    }

    info!(
        ?att_id,
        ?old_status,
        ?new_status,
        ?notifs,
        "Updating event in calendar",
    );

    api.update_calendar_event_attendee_status(
        &event.calendar.id,
        &event.raw.id,
        att_id,
        new_status,
        &answer.now,
    )
    .await?;

    api.update_calendar_event_personal_part(&event.calendar.id, &event.raw.id, color, notifs)
        .await?;

    // Modify the object as well, in case user re-replies without refreshing the
    // view
    *old_status = new_status;

    Ok(())
}

/// Checks whether we should update the notifications (alerts) for this event.
///
/// When user answers "yes" for the first time, we create the alerts; when user
/// has already answered "yes" and now they change the status to "no", we delete
/// the alerts etc.
fn prepare_notifs(
    old_status: CalendarAttendeeStatus,
    new_status: CalendarAttendeeStatus,
    has_notifs: bool,
) -> CalendarNotificationsUpdate {
    // Case 1: `Maybe -> Yes`, `Unanswered -> No` etc.
    //
    // In those transitions we don't update the notifications, because they
    // either are required *and* have been already setup before, or vice versa.
    if old_status.should_notify() == new_status.should_notify() {
        return CalendarNotificationsUpdate::Skip;
    }

    // Case 2: `Unanswered -> Yes`, `No -> Maybe` etc.
    //
    // In those transitions we create the notifications, but only if the event
    // doesn't already have them as we don't want to reset notifications that
    // user has manually entered for this event before.
    if new_status.should_notify() {
        return if has_notifs {
            CalendarNotificationsUpdate::Skip
        } else {
            CalendarNotificationsUpdate::SetToDefault
        };
    }

    // Case 3: `Yes -> No`, `Maybe -> Unanswered` etc., where the event has been
    // effectively discarded and the user shouldn't be notified.
    CalendarNotificationsUpdate::SetTo(Vec::new())
}

/// Sends an email notifying organizer about our status.
#[instrument(skip_all)]
#[allow(clippy::needless_lifetimes)] // false-positive
async fn notify<'a, P, M>(
    api: &Proton,
    pgp: &P,
    sender: M,
    event: &RsvpEvent,
    answer: &RsvpAnswer<'_>,
    decryptor: &CalendarEventDecryptor<'a, P>,
) -> RsvpAnswerResult<(), M>
where
    P: PGPProviderSync,
    M: RsvpMailSender,
{
    info!(
        organizer=?event.organizer.email,
        "Notifying organizer",
    );

    let body = {
        let verb = match answer.status {
            RsvpAnswerStatus::Maybe => "tentatively accepted",
            RsvpAnswerStatus::No => "declined",
            RsvpAnswerStatus::Yes => "accepted",
        };

        let summary = event.summary.as_deref().unwrap_or("(no title)");

        format!("{} {verb} your invitation to {}", answer.email, summary)
    };

    let ics = build_ics(api, pgp, event, answer, decryptor).await?;

    sender
        .send(&event.organizer.email, &body, &ics)
        .await
        .map_err(RsvpAnswerError::Mail)?;

    Ok(())
}

/// Builds an `invite.ics` file that's attached to email sent to the organizer.
async fn build_ics<P>(
    api: &Proton,
    pgp: &P,
    event: &RsvpEvent,
    answer: &RsvpAnswer<'_>,
    decryptor: &CalendarEventDecryptor<'_, P>,
) -> RsvpResult<String>
where
    P: PGPProviderSync,
{
    let prodid = ical::utils::prodid();
    let event = build_ics_event(pgp, event, answer, decryptor)?;
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
    event: &RsvpEvent,
    answer: &RsvpAnswer,
    decryptor: &CalendarEventDecryptor<P>,
) -> RsvpResult<ical::VEvent>
where
    P: PGPProviderSync,
{
    let mut lhs = ical::VEvent::default();

    // Event data is split into the clear-text part (like uid and dates) and the
    // encrypted part (like summary and location) - to construct the reply, we
    // need to join info from both of them.
    //
    // Note that we don't merge all of the fields, e.g. we don't care about the
    // alarms.
    for rhs in &event.raw.shared_events {
        let rhs = rhs.decrypt_and_parse(pgp, decryptor)?;

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

    let dtstamp = answer.now.in_tz("UTC")?;
    let dtstamp = ical::DateTime::try_from(dtstamp)?;

    let attendee = ical::EmailAddress::from(answer.email);
    let attendee = ical::Attendee::from(attendee).with_partstat(answer.status.into());

    Ok(lhs.with_dtstamp(dtstamp).with_attendee(attendee))
}

async fn build_ics_timezones(
    api: &Proton,
    event: &ical::VEvent,
) -> RsvpResult<Vec<ical::VTimeZone>> {
    let dtstart = event.dtstart.as_ref().map(|dtstart| &dtstart.value);
    let dtend = event.dtend.as_ref().map(|dtend| &dtend.value);

    // Step 1: Check which time zones we need (e.g. "Europe/London").
    //
    // Most of the time this will yield just one time zone, but in principle an
    // event can end in a different time zone than the time zone it starts on.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_notifs() {
        // Case 1
        for status in [
            CalendarAttendeeStatus::Unanswered,
            CalendarAttendeeStatus::Maybe,
            CalendarAttendeeStatus::No,
            CalendarAttendeeStatus::Yes,
        ] {
            assert_eq!(
                CalendarNotificationsUpdate::Skip,
                super::prepare_notifs(status, status, false)
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
                    super::prepare_notifs(old_status, new_status, true)
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
                    super::prepare_notifs(old_status, new_status, false)
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
                    super::prepare_notifs(old_status, new_status, false)
                );
            }
        }
    }
}
