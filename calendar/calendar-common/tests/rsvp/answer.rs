use crate::{
    ATTENDEES_EVENT, BAR_ATTENDEE_ID, BAR_ATTENDEE_TOKEN, CALENDAR_ID, EVENT_ID, EVENT_UID,
    FOO_ATTENDEE_ID, FOO_ATTENDEE_TOKEN, INVITE, RsvpEventIdExt, world,
};
use indoc::indoc;
use itertools::Itertools;
use jiff::civil::Weekday;
use pretty_assertions as pa;
use proton_calendar_api::{
    CalendarAttendeeStatus, CalendarNotificationsUpdate, ProtonCalendarMock,
};
use proton_calendar_common::{RsvpAnswer, RsvpEventId, RsvpMailSender};
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
    expected_mail: "bar@localhost accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Yes,
};

const TEST_MAYBE: fn() -> TestCase = || TestCase {
    answer: RsvpAnswer::Maybe,
    expected_ics: "TENTATIVE",
    expected_mail: "bar@localhost tentatively accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Maybe,
};

const TEST_NO: fn() -> TestCase = || TestCase {
    answer: RsvpAnswer::No,
    expected_ics: "DECLINED",
    expected_mail: "bar@localhost declined your invitation to some title",
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
            &world.address_keys,
            &world.cache,
            &world.now,
            "bar@localhost",
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
            "gWfsHvDg",
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
            &world.address_keys,
            &world.cache,
            sender,
            &world.now,
            case.answer,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "foo@localhost".into(),
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
                ATTENDEE;PARTSTAT=%partstat%:mailto:bar@localhost
                END:VEVENT
                END:VCALENDAR
            "}
            .replace("%partstat%", case.expected_ics),
        }),
        mail
    );
}

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
            .with_attendee(
                BAR_ATTENDEE_ID,
                BAR_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Maybe,
            )
            .with_attendee(
                FOO_ATTENDEE_ID,
                FOO_ATTENDEE_TOKEN,
                CalendarAttendeeStatus::Yes,
            )
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #0, answered "yes"
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
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
    });

    // Single edit #1, answered "no"
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
        ORGANIZER:mailto:foo@localhost
        ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@localhost
        END:VEVENT
        END:VCALENDAR
    "};

    let mut event = RsvpEventId::invite(INVITE)
        .fetch(
            &world.sess,
            &world.pgp,
            &world.address_keys,
            &world.cache,
            &world.now,
            "bar@localhost",
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

    // Answer to the first single edit remains unchanged (Yes -> Yes), but the
    // second single edit gets reset from `No` to `Unanswered`
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
            &world.address_keys,
            &world.cache,
            sender,
            &world.now,
            RsvpAnswer::Yes,
        )
        .await
        .unwrap();

    pa::assert_eq!(
        Some(FakeRsvpMail {
            to: "foo@localhost".into(),
            body: "bar@localhost accepted your invitation to ice bucket challenge".into(),
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
                ATTENDEE;PARTSTAT=ACCEPTED:mailto:bar@localhost
                RRULE:FREQ=DAILY
                END:VEVENT
                END:VCALENDAR
            "}
        }),
        mail
    );
}

struct FakeRsvpMailSender<'a>(&'a mut Option<FakeRsvpMail>);

impl RsvpMailSender for FakeRsvpMailSender<'_> {
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
