use chrono::DateTime;
use indoc::indoc;
use pretty_assertions as pa;
use proton_calendar_api::{
    CalendarAttendee, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent,
    CalendarEventPayload, CalendarEventPayloadType, CalendarKey, CalendarKeyFlags, CalendarMember,
    CalendarMemberPassphrase, CalendarPassphrase, ProtonCalendarMock,
};
use proton_calendar_common::{
    RsvpAttendee, RsvpCalendar, RsvpError, RsvpEvent, RsvpEventId, RsvpOccurrence, RsvpOrganizer,
};
use proton_core_api::session::{Config, Session};
use proton_core_common::test_utils::test_context::{MockApiEnv, TestContext};
use proton_crypto::crypto::{KeyGeneratorAlgorithm, PGPProviderSync};
use proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::keys::{
    KeyFlag, KeyId, LocalAddressKey, LocalUserKey, UnlockedAddressKeys,
};
use proton_crypto_account::salts::KeySalt;
use proton_crypto_calendar::{CalendarEventEncryptor, KeyPacket, UnlockedCalendarKey};
use std::sync::Arc;

const SHARED_EVENT: &str = indoc! {"
    BEGIN:VCALENDAR
    VERSION:2.0
    PRODID:-//Proton AG//web-calendar 5.0.47.3//EN
    BEGIN:VEVENT
    UID:IAni7dazrh7RFc_rbQ1c1m4K3JEQ@proton.me
    DTSTAMP:20250423T082009Z
    DESCRIPTION:some description
    SUMMARY:some title
    LOCATION:some location
    END:VEVENT
    END:VCALENDAR
"};

const ATTENDEES_EVENT: &str = indoc! {"
    BEGIN:VCALENDAR
    VERSION:2.0
    PRODID:-//Proton AG//web-calendar 5.0.48.1//EN
    BEGIN:VEVENT
    UID:1Gax95xN@proton.me
    ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@localhost
    ATTENDEE;CN=bar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@localhost
    END:VEVENT
    END:VCALENDAR
"};

/// Make sure we can understand RSVPs that have been auto-imported into the
/// calendar, but haven't been replied to yet.
///
/// Such events are encrypted using just the address key.
#[tokio::test]
async fn using_address_key() {
    let world = world().await;
    let event = world.event("address-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event()), actual);
}

/// Make sure we can understand RSVPs that have been accepted/rejected/maybied.
///
/// Such events get re-encrypted using the calendar key, which requires going
/// through different crypto code paths.
#[tokio::test]
async fn using_shared_key() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
        .await
        .unwrap();

    pa::assert_eq!(Some(expected_event()), actual);
}

/// Make sure we can fetch recurring events - those are identified by an extra
/// header and require passing an extra query parameter for the backend.
#[tokio::test]
async fn recurring() {
    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", Some("Lm9wZW5w"), Some(event))
        .await;

    let actual =
        RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa"), ("X-PM-RECURRENCEID", "Lm9wZW5w")])
            .unwrap()
            .fetch(&world.sess, &world.pgp, &world.address_keys)
            .await
            .unwrap();

    pa::assert_eq!(Some(expected_event()), actual);
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
        .mock_get_calendar_event("8maQ3qBa", None, None)
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
        .await
        .unwrap();

    assert!(actual.is_none());
}

#[tokio::test]
async fn err_unknown_attendee_status() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        PRODID:-//Proton AG//web-calendar 5.0.48.1//EN
        BEGIN:VEVENT
        UID:1Gax95xN@proton.me
        ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@localhost
        ATTENDEE;CN=bar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@localhost
        ATTENDEE;CN=zar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=a06bf6c2:mailto:zar@localhost
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
        .await
        .unwrap_err();

    // Attendee `zar@localhost` is not present in the `CalendarEvent`
    assert_eq!(
        RsvpError::AttendeeHasUnknownStatus.to_string(),
        actual.to_string()
    );
}

#[tokio::test]
async fn err_missing_x_pm_token() {
    const ATTENDEES_EVENT: &str = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        PRODID:-//Proton AG//web-calendar 5.0.48.1//EN
        BEGIN:VEVENT
        UID:1Gax95xN@proton.me
        ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE:mailto:foo@localhost
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
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
        PRODID:-//Proton AG//web-calendar 5.0.48.1//EN
        BEGIN:VEVENT
        UID:q6tHm9Uy@proton.me
        END:VEVENT
        BEGIN:VEVENT
        UID:USfQN64P@proton.me
        END:VEVENT
        END:VCALENDAR
    "};

    let world = world().await;
    let event = world.event("calendar-key", SHARED_EVENT, ATTENDEES_EVENT);

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_bootstrap("HzNtbT1J", world.bootstrap())
        .await;

    world
        .ctx
        .mock_web_server
        .mock_get_calendar_event("8maQ3qBa", None, Some(event))
        .await;

    let actual = RsvpEventId::from_external([("X-PM-UID", "8maQ3qBa")])
        .unwrap()
        .fetch(&world.sess, &world.pgp, &world.address_keys)
        .await
        .unwrap_err();

    assert_eq!(
        RsvpError::IcsContainsMoreThanOneEvent.to_string(),
        actual.to_string()
    );
}

struct World<P>
where
    P: PGPProviderSync,
{
    ctx: Arc<TestContext>,
    sess: Session,
    pgp: P,
    address_keys: UnlockedAddressKeys<P>,
    calendar_key: UnlockedCalendarKey<P>,
}

async fn world() -> World<impl PGPProviderSync> {
    let ctx = TestContext::new().await;

    let sess = {
        let env = MockApiEnv::new(ctx.mock_server().uri()).with_path("/api");
        let cfg = Config::for_env(env);

        Session::builder().with_config(&cfg).build().await.unwrap()
    };

    let pgp = new_pgp_provider();
    let srp = new_srp_provider();

    let user_key = {
        let key_secret = KeySalt::generate()
            .salted_key_passphrase(&srp, "password".as_bytes())
            .unwrap();

        LocalUserKey::generate(&pgp, KeyGeneratorAlgorithm::default(), &key_secret)
            .unwrap()
            .unlock_and_assign_key_id(&pgp, KeyId(String::default()), &key_secret)
            .unwrap()
    };

    let address_keys = UnlockedAddressKeys::from(
        LocalAddressKey::generate(
            &pgp,
            "someone@localhost",
            KeyGeneratorAlgorithm::default(),
            KeyFlag::default(),
            true,
            &user_key,
        )
        .unwrap()
        .unlock_and_assign_key_id(&pgp, KeyId(String::new()), &user_key)
        .unwrap(),
    );

    let calendar_key = UnlockedCalendarKey::generate(&pgp).unwrap();

    World {
        ctx,
        sess,
        pgp,
        address_keys,
        calendar_key,
    }
}

impl<P> World<P>
where
    P: PGPProviderSync,
{
    fn bootstrap(&self) -> CalendarBootstrap {
        let key = self
            .calendar_key
            .export(&self.pgp, &self.address_keys[0])
            .unwrap();

        CalendarBootstrap {
            keys: vec![CalendarKey {
                id: "msY6Hh3D".into(),
                private_key: key.key().into(),
                flags: CalendarKeyFlags::ActiveAndPrimary,
            }],
            passphrase: CalendarPassphrase {
                member_passphrases: vec![CalendarMemberPassphrase {
                    member_id: "qyomV2nX".into(),
                    passphrase: key.passphrase().into(),
                    signature: key.signature().into(),
                }],
            },
            members: [CalendarMember {
                id: "qyomV2nX".into(),
                name: "My calendar".into(),
                color: "#273EB2".into(),
            }],
        }
    }

    fn event(&self, mode: &str, shared_event: &str, attendees_event: &str) -> CalendarEvent {
        let encryptor = match mode {
            "address-key" => {
                CalendarEventEncryptor::for_address(&self.pgp, &self.address_keys).unwrap()
            }
            "calendar-key" => CalendarEventEncryptor::for_calendar(
                &self.pgp,
                &self.address_keys,
                &self.calendar_key,
            )
            .unwrap(),
            _ => unreachable!(),
        };

        let (shared_event, _) = encryptor
            .encrypt(&self.pgp, shared_event.as_bytes())
            .unwrap();

        let (attendees_event, _) = encryptor
            .encrypt(&self.pgp, attendees_event.as_bytes())
            .unwrap();

        let key_packets = encryptor.finish(&self.pgp).unwrap();
        let address_key_packet = key_packets.address_key_packet.map(KeyPacket::into_base64);
        let shared_key_packet = key_packets.shared_key_packet.map(KeyPacket::into_base64);

        CalendarEvent {
            shared_events: vec![CalendarEventPayload {
                ty: CalendarEventPayloadType::Encrypted,
                data: shared_event.into_base64(),
                signature: None,
                author: "foo@localhost".into(),
            }],
            calendar_id: "HzNtbT1J".into(),
            start_time: 1_744_790_400,
            end_time: 1_744_795_800,
            full_day: false,
            recurrence_id: None,
            address_key_packet,
            shared_key_packet,
            attendees_events: [CalendarEventPayload {
                ty: CalendarEventPayloadType::Encrypted,
                data: attendees_event.into_base64(),
                signature: None,
                author: "foo@localhost".into(),
            }],
            attendees: vec![
                CalendarAttendee {
                    id: "gWfsHvDg".into(),
                    token: "d15cf90c".into(),
                    status: CalendarAttendeeStatus::Unanswered,
                },
                CalendarAttendee {
                    id: "V3FdcecX".into(),
                    token: "245902dc".into(),
                    status: CalendarAttendeeStatus::Maybe,
                },
            ],
        }
    }
}

fn expected_event() -> RsvpEvent {
    RsvpEvent {
        summary: "some title".into(),
        location: Some("some location".into()),
        description: Some("some description".into()),
        occurrence: RsvpOccurrence::DateTime {
            starts_at: DateTime::from_timestamp(1_744_790_400, 0).unwrap(),
            ends_at: DateTime::from_timestamp(1_744_795_800, 0).unwrap(),
        },
        attendees: vec![
            RsvpAttendee {
                email: "foo@localhost".into(),
                status: CalendarAttendeeStatus::Maybe,
            },
            RsvpAttendee {
                email: "bar@localhost".into(),
                status: CalendarAttendeeStatus::Unanswered,
            },
        ],
        organizer: RsvpOrganizer {
            email: "foo@localhost".into(),
        },
        calendar: RsvpCalendar {
            name: "My calendar".into(),
            color: "#273EB2".into(),
        },
    }
}
