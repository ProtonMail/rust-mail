use crate::{
    BAR_ATTENDEE_ID, BAR_ATTENDEE_TOKEN, CALENDAR_ID, EVENT_ID, EVENT_UID, FOO_ATTENDEE_ID,
    FOO_ATTENDEE_TOKEN, INVITE, RsvpEventIdExt, SHARED_EVENT, expected_event,
    expected_offline_event, world,
};
use indoc::indoc;
use jiff::{Zoned, civil::Weekday};
use pretty_assertions as pa;
use proton_calendar_api::{CalendarAttendee, CalendarAttendeeStatus, ProtonCalendarMock};
use proton_calendar_common::{
    RsvpAttendee, RsvpError, RsvpEventId, RsvpFetchApiError, RsvpFetchError, RsvpIntent,
    RsvpOrganizer, RsvpProgress, RsvpRecency, RsvpRelation,
};
use proton_core_api::{
    session::{Config, Session},
    status_observer::StatusObserver,
    status_watcher::StatusWatcher,
};
use proton_core_common::test_utils::test_context::MockApiEnv;
use proton_ical as ical;
use std::str::FromStr;

/// Make sure we can understand RSVPs that have been auto-imported into the
/// calendar, but haven't been replied to yet.
///
/// Such events are encrypted using just the address key.
#[tokio::test]
async fn using_address_key() {
    let world = world().await;
    let event = world.event(|event| event.basic().using_address_key());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(RsvpIntent::Invite, event)), actual);
}

/// Make sure we can understand RSVPs that have been accepted/rejected/maybied.
///
/// Such events get re-encrypted using the calendar key, which requires going
/// through different crypto code paths.
#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn using_calendar_key() {
    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(RsvpIntent::Invite, event)), actual);
}

/// Make sure we can fetch recurring events - those are identified by the
/// presence of `RECURRENCE-ID` and require passing an extra query parameter
/// when we ask backend about it.
#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn recurring() {
    const INVITE: &str = indoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T133000Z
        RECURRENCE-ID:20180101T123000Z
        DESCRIPTION:some description
        SUMMARY:some title
        LOCATION:some location
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic());

    let rid = Zoned::from_str("20180101T123000[UTC]")
        .unwrap()
        .timestamp()
        .as_second();

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, Some(rid), vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event(RsvpIntent::Invite, event)), actual);
}

#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn reminder() {
    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event(EVENT_UID, EVENT_ID, event.clone())
        .await;

    let actual = RsvpEventId::reminder(EVENT_UID, EVENT_ID)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpIntent::Reminder, actual.intent);
}

#[tokio::test]
async fn reminder_without_attendees() {
    let world = world().await;

    let event = world.event(|event| {
        event
            .with_id(EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_shared_event(SHARED_EVENT)
    });

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event(EVENT_UID, EVENT_ID, event.clone())
        .await;

    let actual = RsvpEventId::reminder(EVENT_UID, EVENT_ID)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpIntent::Reminder, actual.intent);
}

#[tokio::test]
async fn alias() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        ATTENDEE;CN=foo@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@pm.me
        ATTENDEE;CN=bar+spam@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar+spam@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic().with_attendees_event(ATTENDEES_EVENT));

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event(EVENT_UID, EVENT_ID, event.clone())
        .await;

    let actual = RsvpEventId::reminder(EVENT_UID, EVENT_ID)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpIntent::Reminder, actual.intent);
}

#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn outdated() {
    const INVITE: &str = indoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T060000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T133000Z
        DESCRIPTION:some old description
        SUMMARY:some old title
        LOCATION:some old location
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(RsvpRecency::Outdated, actual.recency);
}

/// Make sure we can fetch cancelled events *and* mark them as such; this
/// requires parsing `Event.CalendarEvents[]`.
#[tokio::test]
async fn cancelled() {
    const CALENDAR_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T080000Z
        STATUS:CANCELLED
        TRANSP:OPAQUE
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic().with_calendar_event(CALENDAR_EVENT));

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!actual.can_be_answered());
    assert_eq!(RsvpProgress::Cancelled, actual.progress);
}

/// Make sure that asking for a non-auto-imported-imported event reads data from
/// `invite.ics`.
///
/// Users can disable auto-importing RSVPs, in which case asking the calendar
/// about that particular event will return "whoopsie, what's that?" - this test
/// makes sure we can handle this scenario (probably like 0.1% of users though).
#[tokio::test]
async fn missing_event() {
    let world = world().await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, Vec::new())
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    pa::assert_eq!(
        expected_offline_event(RsvpFetchApiError::EventMissing),
        actual
    );

    // We don't support creating events in the calendar, so non-auto-imported
    // can't be answered yet
    assert!(!actual.can_be_answered());
}

/// Make sure that we fail gracefully if there's no internet connection.
#[tokio::test]
async fn network_failure() {
    let mut world = world().await;

    world.sess = {
        let env = MockApiEnv::new("http://localhost:1");
        let cfg = Config::for_env(env);

        let status =
            StatusWatcher::with_observer(StatusObserver::test(world.ctx.context.spawner()));

        Session::builder()
            .with_config(&cfg)
            .with_status(status)
            .build(world.ctx.context.spawner())
            .await
            .unwrap()
    };

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    pa::assert_eq!(
        expected_offline_event(RsvpFetchApiError::NetworkFailure),
        actual
    );

    assert!(!actual.can_be_answered());
}

#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn user_is_organizer() {
    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "foo@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!actual.can_be_answered());
    assert_eq!(None, actual.user_attendee());
    assert_eq!(RsvpRelation::Organizer, actual.relation);
}

#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn user_is_party_crasher() {
    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "root@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!actual.can_be_answered());
    assert_eq!(None, actual.user_attendee());
    assert_eq!(RsvpRelation::PartyCrasher, actual.relation);
}

/// Make sure we fall back to organizer's another email address if the one
/// present in the invite is invalid.
#[tokio::test]
#[allow(clippy::redundant_closure_for_method_calls, reason = "false-positive")]
async fn invalid_organizer() {
    const INVITE: &str = indoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T133000Z
        DESCRIPTION:some description
        SUMMARY:some title
        LOCATION:some location
        ORGANIZER:mailto:g'man
        ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "foo@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        RsvpOrganizer {
            name: Some("Foo Localhosty".into()),
            reply_email: "foo@pm.me".into(),
            display_email: "foo@pm.me".into(),
        },
        actual.organizer
    );
}

/// Make sure we can handle `ORGANIZER;EMAIL=...`, where the invitation provides
/// a different "display email" and "reply email" for the organizer.
#[tokio::test]
async fn obfuscated_organizer() {
    const INVITE: &str = indoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T133000Z
        DESCRIPTION:some description
        SUMMARY:some title
        LOCATION:some location
        ORGANIZER;EMAIL=foo@pm.me:mailto:mcw2Yd8t@secret
        ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        ATTENDEE;CN=Foo;EMAIL=foo@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:mcw2Yd8t@secret
        ATTENDEE;CN=Bar;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;

    let event = world.event(|event| {
        event
            .with_id(EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
            .with_attendees(vec![
                CalendarAttendee {
                    id: BAR_ATTENDEE_ID.into(),
                    token: BAR_ATTENDEE_TOKEN.into(),
                    status: CalendarAttendeeStatus::Unanswered,
                },
                CalendarAttendee {
                    id: FOO_ATTENDEE_ID.into(),
                    token: FOO_ATTENDEE_TOKEN.into(),
                    status: CalendarAttendeeStatus::Maybe,
                },
            ])
    });

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "foo@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        RsvpOrganizer {
            name: Some("Foo Localhosty".into()),
            reply_email: "mcw2Yd8t@secret".into(),
            display_email: "foo@pm.me".into(),
        },
        actual.organizer
    );

    assert_eq!(
        vec![RsvpAttendee {
            id: Some(BAR_ATTENDEE_ID.into()),
            token: Some(BAR_ATTENDEE_TOKEN.into()),
            name: Some("Bar Localhosty".into()),
            email: "bar@pm.me".into(),
            status: Some(CalendarAttendeeStatus::Unanswered),
            role: ical::Role::ReqParticipant,
        }],
        actual.attendees
    );
}

#[tokio::test]
async fn err_unknown_attendee() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        ATTENDEE;CN=foo@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@pm.me
        ATTENDEE;CN=bar@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@pm.me
        ATTENDEE;CN=zar@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=a06bf6c2:mailto:zar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic().with_attendees_event(ATTENDEES_EVENT));

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap_err();

    // Attendee `zar@pm.me` is not present in the `CalendarEvent`
    assert!(matches!(
        actual,
        RsvpFetchError::Rsvp(RsvpError::UnknownAttendee),
    ));
}

#[tokio::test]
async fn err_missing_x_pm_token() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        ATTENDEE;CN=bar@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE:mailto:bar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event(|event| event.basic().with_attendees_event(ATTENDEES_EVENT));

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap_err();

    assert!(matches!(
        actual,
        RsvpFetchError::Rsvp(RsvpError::AttendeeHasNoXPmToken),
    ));
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
    let event = world.event(|event| event.basic().with_attendees_event(ATTENDEES_EVENT));

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event])
        .await;

    let actual = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            &world.contacts,
            &world.now,
            "bar@pm.me",
            Weekday::Monday,
        )
        .await
        .unwrap_err();

    assert!(matches!(
        actual,
        RsvpFetchError::Rsvp(RsvpError::IcsContainsMoreThanOneEvent),
    ));
}
