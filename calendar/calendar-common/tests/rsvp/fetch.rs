use crate::{ATTENDEES_EVENT, SHARED_EVENT, expected_event, world};
use indoc::indoc;
use jiff::Zoned;
use pretty_assertions as pa;
use proton_calendar_api::ProtonCalendarMock;
use proton_calendar_common::{RsvpError, RsvpEventId, RsvpEventType, RsvpStatus};
use std::str::FromStr;

/// Make sure we can understand RSVPs that have been auto-imported into the
/// calendar, but haven't been replied to yet.
///
/// Such events are encrypted using just the address key.
#[tokio::test]
async fn using_address_key() {
    let world = world().await;
    let event = world.event("address-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event.clone()))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(event)), actual);
}

/// Make sure we can understand RSVPs that have been accepted/rejected/maybied.
///
/// Such events get re-encrypted using the calendar key, which requires going
/// through different crypto code paths.
#[tokio::test]
async fn using_shared_key() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event.clone()))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(event)), actual);
}

/// Make sure we can fetch recurring events - those are identified by an extra
/// header and require passing an extra query parameter for the backend.
#[tokio::test]
async fn recurring() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    let rid = Zoned::from_str("20250423T082000[UTC]")
        .unwrap()
        .timestamp()
        .as_second();

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", Some(rid), Some(event.clone()))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", Some(rid))
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(event)), actual);
}

/// Make sure we can fetch events with direct id - see [`RsvpEventId::Direct`].
#[tokio::test]
async fn direct_invite() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", "pFmwNlJp", event.clone())
        .await;

    let actual = RsvpEventId::direct("8maQ3qBa", "pFmwNlJp", RsvpEventType::Invite)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(event)), actual);
}

#[tokio::test]
async fn direct_reminder() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", "pFmwNlJp", event.clone())
        .await;

    let actual = RsvpEventId::direct("8maQ3qBa", "pFmwNlJp", RsvpEventType::Reminder)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpEventType::Reminder, actual.ty);
}

/// Make sure we can fetch cancelled events *and* mark them as such; this
/// requires parsing `Event.CalendarEvents[]`.
#[tokio::test]
async fn cancelled() {
    const CALENDAR_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:1Gax95xN@proton.me
        DTSTAMP:20250423T082009Z
        STATUS:CANCELLED
        TRANSP:OPAQUE
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;

    let event = world.event(
        "calendar-key",
        SHARED_EVENT,
        ATTENDEES_EVENT,
        Some(CALENDAR_EVENT),
    );

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event.clone()))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpStatus::Cancelled, actual.status);
}

/// Make sure that asking for a non-imported event doesn't end up as error.
///
/// Users can disable auto-importing RSVPs, in which case asking the calendar
/// about that particular event will return "whoopsie, what's that?" - this test
/// makes sure we can handle this scenario (probably like 0.1% of users).
#[tokio::test]
async fn unknown() {
    let world = world().await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, None)
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap();

    assert!(actual.is_none());
}

#[tokio::test]
async fn err_unknown_attendee_status() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:1Gax95xN@proton.me
        ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@localhost
        ATTENDEE;CN=bar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@localhost
        ATTENDEE;CN=zar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=a06bf6c2:mailto:zar@localhost
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap_err();

    // Attendee `zar@localhost` is not present in the `CalendarEvent`
    assert_eq!(
        RsvpError::AttendeeIsNotKnown.to_string(),
        actual.to_string()
    );
}

#[tokio::test]
async fn err_missing_x_pm_token() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:1Gax95xN@proton.me
        ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE:mailto:foo@localhost
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap_err();

    assert_eq!(
        RsvpError::AttendeeHasNoXPmToken.to_string(),
        actual.to_string()
    );
}

#[tokio::test]
async fn err_many_events_in_ics() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:q6tHm9Uy@proton.me
        END:VEVENT
        BEGIN:VEVENT
        UID:USfQN64P@proton.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT, None);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::indirect("8maQ3qBa", None)
        .fetch(&world.sess, &world.pgp, &world.address_keys, &world.cache)
        .await
        .unwrap_err();

    assert_eq!(
        RsvpError::IcsContainsMoreThanOneEvent.to_string(),
        actual.to_string()
    );
}
