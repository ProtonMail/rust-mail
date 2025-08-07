use crate::{
    ATTENDEES_EVENT, BAR_ATTENDEE_ID, BAR_ATTENDEE_TOKEN, CALENDAR_ID, EVENT_ID, EVENT_UID,
    FOO_ATTENDEE_ID, FOO_ATTENDEE_TOKEN, INVITE, RsvpEventIdExt, ZAR_ATTENDEE_ID,
    ZAR_ATTENDEE_TOKEN, world,
};
use indoc::indoc;
use itertools::Itertools;
use jiff::civil::Weekday;
use pretty_assertions as pa;
use proton_calendar_api::{
    CalendarAttendeeStatus, CalendarNotificationsUpdate, ProtonCalendarMock,
};
use proton_calendar_common::{RsvpAnswer, RsvpEventId, RsvpMail};
use proton_ical::ics;
use std::io;
use test_case::test_case;

struct TestCase {
    answer: RsvpAnswer,
    expected_ics: &'static str,
    expected_mail: &'static str,
    expected_notifs: CalendarNotificationsUpdate,
    expected_status: CalendarAttendeeStatus,
}

const TEST_YES: fn() -> TestCase = || TestCase {
    answer: RsvpAnswer::Yes,
    expected_ics: "ACCEPTED",
    expected_mail: "bar@pm.me accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Yes,
};

const TEST_MAYBE: fn() -> TestCase = || TestCase {
    answer: RsvpAnswer::Maybe,
    expected_ics: "TENTATIVE",
    expected_mail: "bar@pm.me tentatively accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Maybe,
};

const TEST_NO: fn() -> TestCase = || TestCase {
    answer: RsvpAnswer::No,
    expected_ics: "DECLINED",
    expected_mail: "bar@pm.me declined your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::Skip,
    expected_status: CalendarAttendeeStatus::No,
};

#[test_case(TEST_YES)]
#[test_case(TEST_MAYBE)]
#[test_case(TEST_NO)]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn basic(case: fn() -> TestCase) {
    let case = case();
    let world = world().await;
    let event = world.event(|event| event.basic().using_address_key());

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap_ex(CALENDAR_ID, world.bootstrap(), |mock| mock.expect(2))
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let mut event = RsvpEventId::invite(INVITE)
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

    // ---

    let mut mail = None;

    world
        .ctx
        .mock_web_server
        .mock_upgrade_calendar_event_invite(CALENDAR_ID, EVENT_ID)
        .await;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_attendee_status(
            CALENDAR_ID,
            EVENT_ID,
            BAR_ATTENDEE_ID,
            case.expected_status,
            &world.now,
        )
        .await;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_personal_part(
            CALENDAR_ID,
            EVENT_ID,
            Some("#aabbcc"),
            case.expected_notifs,
        )
        .await;

    let sender = FakeRsvpMailSender(&mut mail);

    event
        .answer(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            sender,
            &world.now,
            case.answer,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "foo@pm.me".into(),
            body: case.expected_mail.into(),
            ics: ics! {"
                BEGIN:VCALENDAR
                METHOD:REPLY
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:8maQ3qBa
                DTSTAMP:20180101T100000Z
                DTSTART:20180101T120000Z
                DTEND:20180101T133000Z
                SUMMARY:some title
                LOCATION:some location
                ATTENDEE;PARTSTAT=%partstat%:mailto:bar@pm.me
                END:VEVENT
                END:VCALENDAR
            "}
            .replace("%partstat%", case.expected_ics),
        }),
        mail
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
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
        .mock_get_calendar_bootstrap_ex(CALENDAR_ID, world.bootstrap(), |mock| mock.expect(2))
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event.clone()])
        .await;

    let mut event = RsvpEventId::invite(INVITE)
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

    // ---

    let mut mail = None;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_attendee_status(
            CALENDAR_ID,
            EVENT_ID,
            BAR_ATTENDEE_ID,
            CalendarAttendeeStatus::Yes,
            &world.now,
        )
        .await;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_personal_part(
            CALENDAR_ID,
            EVENT_ID,
            Some("#aabbcc"),
            CalendarNotificationsUpdate::SetToDefault,
        )
        .await;

    let sender = FakeRsvpMailSender(&mut mail);

    event
        .answer(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            sender,
            &world.now,
            RsvpAnswer::Yes,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "foo@pm.me".into(),
            body: "bar+spam@pm.me accepted your invitation to some title".into(),
            ics: ics! {"
                BEGIN:VCALENDAR
                METHOD:REPLY
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:8maQ3qBa
                DTSTAMP:20180101T100000Z
                DTSTART:20180101T120000Z
                DTEND:20180101T133000Z
                SUMMARY:some title
                LOCATION:some location
                ATTENDEE;PARTSTAT=ACCEPTED:mailto:bar+spam@pm.me
                END:VEVENT
                END:VCALENDAR
            "}
        }),
        mail
    );
}

/// Make sure that changing answer on a recurring event resets single edits with
/// different answers.
///
/// We're given the following events:
///
/// - ice bucket challenge
///   (recurring event, answered `Maybe`)
///
/// - ice bucket challenge with eminem
///   (single edit, answered `Yes`)
///
/// - ice bucket challenge with vsauce
///   (single edit, answered `No`)
///
/// - ice bucket challenge with linus
///   (single edit, unanswered)
///
/// - ice bucket challenge with bill
///   (single edit, cancelled)
///
/// We then change the answer on the recurring event to `Yes` which should
/// cascade to the single edits.
#[tokio::test]
#[allow(clippy::items_after_statements)]
#[allow(clippy::too_many_lines)]
async fn recurring_with_single_edits() {
    const PARENT_EVENT_ID: &str = "bwcmICei";
    const CHILD0_EVENT_ID: &str = "NTdcFLXh";
    const CHILD1_EVENT_ID: &str = "kkGZO8Ka";
    const CHILD3_EVENT_ID: &str = "8Q8EXdhi";
    const CHILD2_EVENT_ID: &str = "YC5q9KAJ";

    // ---

    let world = world().await;

    let parent = world.event(|event| {
        const SHARED_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180101T080000Z
            DTSTART:20180101T120000Z
            DTEND:20180101T133000Z
            RRULE:FREQ=DAILY
            SUMMARY:ice bucket challenge
            END:VEVENT
            END:VCALENDAR
        "};

        event
            .with_id(PARENT_EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Maybe,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_attendee(
                ZAR_ATTENDEE_ID,
                ZAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #0, answered `Yes`
    let child0 = world.event(|event| {
        const SHARED_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180102T080000Z
            DTSTART:20180102T120000Z
            DTEND:20180102T133000Z
            DESCRIPTION:ice bucket challenge with eminem
            END:VEVENT
            END:VCALENDAR
        "};

        event
            .with_id(CHILD0_EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Yes,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Yes,
            )
            .with_attendee(
                ZAR_ATTENDEE_ID,
                ZAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #1, answered `No`
    let child1 = world.event(|event| {
        const SHARED_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180103T080000Z
            DTSTART:20180103T120000Z
            DTEND:20180103T133000Z
            DESCRIPTION:ice bucket challenge with vsauce
            END:VEVENT
            END:VCALENDAR
        "};

        event
            .with_id(CHILD1_EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Maybe,
            )
            .with_attendee(
                ZAR_ATTENDEE_ID,
                ZAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #2, unanswered (remains unanswered after the update)
    let child2 = world.event(|event| {
        const SHARED_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180104T080000Z
            DTSTART:20180104T120000Z
            DTEND:20180104T133000Z
            DESCRIPTION:ice bucket challenge with linus
            END:VEVENT
            END:VCALENDAR
        "};

        event
            .with_id(CHILD2_EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Unanswered,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Yes,
            )
            .with_attendee(
                ZAR_ATTENDEE_ID,
                ZAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #3, cancelled (gets ignored during the update)
    let child3 = world.event(|event| {
        const SHARED_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180103T080000Z
            DTSTART:20180103T120000Z
            DTEND:20180103T133000Z
            DESCRIPTION:ice bucket challenge with bill
            END:VEVENT
            END:VCALENDAR
        "};

        const CALENDAR_EVENT: &str = indoc! {"
            BEGIN:VCALENDAR
            VERSION:2.0
            BEGIN:VEVENT
            UID:8maQ3qBa
            DTSTAMP:20180103T080000Z
            STATUS:CANCELLED
            TRANSP:OPAQUE
            END:VEVENT
            END:VCALENDAR
        "};

        event
            .with_id(CHILD3_EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Yes,
            )
            .with_attendee(
                ZAR_ATTENDEE_ID,
                ZAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::No,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
            .with_calendar_event(CALENDAR_EVENT)
    });

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap_ex(CALENDAR_ID, world.bootstrap(), |mock| mock.expect(2))
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(
            EVENT_UID,
            None,
            vec![parent, child0, child1, child2, child3],
        )
        .await;

    // ---

    const INVITE: &str = indoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        VERSION:2.0
        BEGIN:VEVENT
        UID:8maQ3qBa
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T133000Z
        RRULE:FREQ=DAILY
        SUMMARY:ice bucket challenge
        ORGANIZER;EMAIL=foo@pm.me:mailto:mcw2Yd8t@secret
        ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@pm.me
        END:VEVENT
        END:VCALENDAR
    "};

    let mut event = RsvpEventId::invite(INVITE)
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

    assert!(event.raw.is_some());
    assert_eq!(4, event.children.len());

    // ---

    let mut mail = None;

    // Answer to the parent event gets changed from `Maybe` to `Yes`
    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_attendee_status(
            CALENDAR_ID,
            PARENT_EVENT_ID,
            BAR_ATTENDEE_ID,
            CalendarAttendeeStatus::Yes,
            &world.now,
        )
        .await;

    // Since the parent event had notifications already set up before[1], we
    // don't expect them to change when the reply is changed to `Yes`
    //
    // [1] our test starts with the parent event having a mocked `Maybe` answer
    //     and in reality answering `Maybe` already creates notifications, i.e.
    //     the code knows that notifications are already supposed to be there
    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_personal_part(
            CALENDAR_ID,
            PARENT_EVENT_ID,
            Some("#aabbcc"),
            CalendarNotificationsUpdate::Skip,
        )
        .await;

    // Answer to the first single edit remains unchanged (`Yes` -> `Yes`), but
    // the second single edit gets reset from `No` to `Unanswered`
    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_attendee_status(
            CALENDAR_ID,
            CHILD1_EVENT_ID,
            BAR_ATTENDEE_ID,
            CalendarAttendeeStatus::Unanswered,
            &world.now,
        )
        .await;

    // Since the second single edit had no notifications set up before[1] and
    // doesn't require to have any notifications now (`Unanswered` status is
    // notification-less, so to say), we don't expect for notifications to
    // change here.
    //
    // [1] our test starts with the second single edit event having a mocked
    //     `No` answer
    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_personal_part(
            CALENDAR_ID,
            CHILD1_EVENT_ID,
            Some("#aabbcc"),
            CalendarNotificationsUpdate::Skip,
        )
        .await;

    let sender = FakeRsvpMailSender(&mut mail);

    event
        .answer(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            sender,
            &world.now,
            RsvpAnswer::Yes,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "mcw2Yd8t@secret".into(),
            body: "bar@pm.me accepted your invitation to ice bucket challenge".into(),
            ics: ics! {"
                BEGIN:VCALENDAR
                METHOD:REPLY
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:8maQ3qBa
                DTSTAMP:20180101T100000Z
                DTSTART:20180101T120000Z
                DTEND:20180101T133000Z
                SUMMARY:ice bucket challenge
                ATTENDEE;PARTSTAT=ACCEPTED:mailto:bar@pm.me
                RRULE:FREQ=DAILY
                END:VEVENT
                END:VCALENDAR
            "}
        }),
        mail
    );

    // Bar's answer on the parent event changes from `Maybe` to `Yes`
    assert_eq!(
        CalendarAttendeeStatus::Yes,
        event
            .raw
            .as_ref()
            .unwrap()
            .attendee_status(&BAR_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Foo's answer on the parent event remains unchanged since they are the
    // organizer
    assert_eq!(
        CalendarAttendeeStatus::No,
        event
            .raw
            .as_ref()
            .unwrap()
            .attendee_status(&FOO_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Bar's answer on the single edit #0 remains unchanged (i.e. not reset)
    // since it already was `Yes`
    assert_eq!(
        CalendarAttendeeStatus::Yes,
        event.children[0]
            .attendee_status(&BAR_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Foo's answer on the parent event remains unchanged since they are the
    // organizer
    assert_eq!(
        CalendarAttendeeStatus::Yes,
        event.children[0]
            .attendee_status(&FOO_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Bar's answer on the single edit #1 gets reset since it was `No` which
    // doesn't match the new answer on the parent event (`Yes`)
    assert_eq!(
        CalendarAttendeeStatus::Unanswered,
        event.children[1]
            .attendee_status(&BAR_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Foo's answer on the single edit #1 remains unchanged since they are the
    // organizer
    assert_eq!(
        CalendarAttendeeStatus::Maybe,
        event.children[1]
            .attendee_status(&FOO_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Bar's answer on the single edit #2 remains unchanged since it already was
    // unanswered
    assert_eq!(
        CalendarAttendeeStatus::Unanswered,
        event.children[2]
            .attendee_status(&BAR_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Foo's answer on the single edit #2 remains unchanged since they are the
    // organizer
    assert_eq!(
        CalendarAttendeeStatus::Yes,
        event.children[2]
            .attendee_status(&FOO_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Bar's answer on the single edit #3 remains unchanged since the event was
    // cancelled
    assert_eq!(
        CalendarAttendeeStatus::No,
        event.children[3]
            .attendee_status(&BAR_ATTENDEE_TOKEN.into())
            .unwrap()
    );

    // Foo's answer on single edit #3 remains unchanged since they are the
    // organizer (plus the event got cancelled)
    assert_eq!(
        CalendarAttendeeStatus::Yes,
        event.children[3]
            .attendee_status(&FOO_ATTENDEE_TOKEN.into())
            .unwrap()
    );
}

/// Make sure we can correctly reply when one UID resolves to events across
/// different calendars.
///
/// This can happen for Proton-to-Proton invites when:
///
/// - organizer creates a shared calendar,
/// - organizer invites an attendee to this shared calendar,
/// - organizer creates an event in the shared calendar, inviting attendee to
///   it.
///
/// When the invitation is sent, the backend auto-imports it into attendee's
/// *default* calendar ("My calendar") - this means that there are, in a way,
/// two events now: one within the shared calendar and another in the attendee's
/// default calendar.
///
/// When replying, we can choose any event as the backend keeps them in sync -
/// what is important for us is that we don't try to reply to both events at
/// once.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn multiple_calendars() {
    const FIRST_CALENDAR_ID: &str = "xI8emESw";
    const FIRST_EVENT_ID: &str = "Tnld9DeP";

    const SECOND_CALENDAR_ID: &str = "9NQ8rRCB";
    const SECOND_EVENT_ID: &str = "DMTwRgJT";

    let world = world().await;

    let first_event = world.event(|event| {
        event
            .basic()
            .with_id(FIRST_EVENT_ID)
            .with_calendar_id(FIRST_CALENDAR_ID)
    });

    let second_event = world.event(|event| {
        event
            .basic()
            .with_id(SECOND_EVENT_ID)
            .with_calendar_id(SECOND_CALENDAR_ID)
    });

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap_ex(
            FIRST_CALENDAR_ID,
            world.bootstrap_ex(FIRST_CALENDAR_ID),
            |mock| mock.expect(2),
        )
        .await;

    world
        .ctx
        .mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![first_event, second_event])
        .await;

    let mut event = RsvpEventId::invite(INVITE)
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

    // ---

    let mut mail = None;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_attendee_status(
            FIRST_CALENDAR_ID,
            FIRST_EVENT_ID,
            BAR_ATTENDEE_ID,
            CalendarAttendeeStatus::Yes,
            &world.now,
        )
        .await;

    world
        .ctx
        .mock_web_server
        .mock_update_calendar_event_personal_part(
            FIRST_CALENDAR_ID,
            FIRST_EVENT_ID,
            Some("#aabbcc"),
            CalendarNotificationsUpdate::SetToDefault,
        )
        .await;

    let sender = FakeRsvpMailSender(&mut mail);

    event
        .answer(
            &world.sess,
            &world.pgp,
            &world.keys,
            &world.cache,
            sender,
            &world.now,
            RsvpAnswer::Yes,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "foo@pm.me".into(),
            body: "bar@pm.me accepted your invitation to some title".into(),
            ics: ics! {"
                BEGIN:VCALENDAR
                METHOD:REPLY
                VERSION:2.0
                CALSCALE:GREGORIAN
                BEGIN:VEVENT
                UID:8maQ3qBa
                DTSTAMP:20180101T100000Z
                DTSTART:20180101T120000Z
                DTEND:20180101T133000Z
                SUMMARY:some title
                LOCATION:some location
                ATTENDEE;PARTSTAT=ACCEPTED:mailto:bar@pm.me
                END:VEVENT
                END:VCALENDAR
            "}
        }),
        mail
    );
}

struct FakeRsvpMailSender<'a>(&'a mut Option<FakeRsvpMail>);

impl RsvpMail for FakeRsvpMailSender<'_> {
    type Error = io::Error;

    async fn send(self, to: &str, body: &str, ics: &str) -> io::Result<()> {
        // PRODID is generated dynamically (it contains app's version), so let's
        // strip it to make the assertion easier
        let ics = ics
            .lines()
            .filter(|line| !line.contains("PRODID"))
            .join("\r\n");

        *self.0 = Some(FakeRsvpMail {
            to: to.to_owned(),
            body: body.to_owned(),
            ics,
        });

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FakeRsvpMail {
    to: String,
    body: String,
    ics: String,
}
