use crate::{CALENDAR_ID, EVENT_ID, EVENT_UID, INVITE, RsvpEventIdExt, world};
use itertools::Itertools;
use jiff::civil::Weekday;
use pretty_assertions as pa;
use proton_calendar_api::{
    CalendarAttendeeStatus, CalendarNotificationsUpdate, ProtonCalendarMock,
};
use proton_calendar_common::{RsvpAnswer, RsvpAnswerStatus, RsvpEventId, RsvpMailSender};
use proton_ical::ics;
use std::io;
use test_case::test_case;

struct TestCase {
    status: RsvpAnswerStatus,
    expected_ics: &'static str,
    expected_mail: &'static str,
    expected_notifs: CalendarNotificationsUpdate,
    expected_status: CalendarAttendeeStatus,
}

const TEST_YES: fn() -> TestCase = || TestCase {
    status: RsvpAnswerStatus::Yes,
    expected_ics: "ACCEPTED",
    expected_mail: "bar@localhost accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Yes,
};

const TEST_MAYBE: fn() -> TestCase = || TestCase {
    status: RsvpAnswerStatus::Maybe,
    expected_ics: "TENTATIVE",
    expected_mail: "bar@localhost tentatively accepted your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::SetToDefault,
    expected_status: CalendarAttendeeStatus::Maybe,
};

const TEST_NO: fn() -> TestCase = || TestCase {
    status: RsvpAnswerStatus::No,
    expected_ics: "DECLINED",
    expected_mail: "bar@localhost declined your invitation to some title",
    expected_notifs: CalendarNotificationsUpdate::Skip,
    expected_status: CalendarAttendeeStatus::No,
};

#[test_case(TEST_YES)]
#[test_case(TEST_MAYBE)]
#[test_case(TEST_NO)]
#[tokio::test]
async fn answer(case: fn() -> TestCase) {
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
            Weekday::Monday,
        )
        .await
        .unwrap()
        .unwrap();

    // ---

    let answer = RsvpAnswer {
        now: "20180101T120000[UTC]".parse().unwrap(),
        email: "bar@localhost",
        status: case.status,
    };

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
            &answer.now,
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
            answer,
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
                DTSTAMP:20180101T120000Z
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
