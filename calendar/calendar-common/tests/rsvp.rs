mod rsvp {
    mod fetch;
}

use chrono::DateTime;
use indoc::indoc;
use proton_calendar_api::{
    CalendarAttendee, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent,
    CalendarEventPayload, CalendarEventPayloadType, CalendarKey, CalendarKeyFlags, CalendarMember,
    CalendarMemberPassphrase, CalendarPassphrase,
};
use proton_calendar_common::{
    RsvpAttendee, RsvpCalendar, RsvpEvent, RsvpOccurrence, RsvpOrganizer,
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

fn expected_event(raw: CalendarEvent) -> RsvpEvent {
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
                id: "V3FdcecX".into(),
                token: "245902dc".into(),
                email: "foo@localhost".into(),
                status: CalendarAttendeeStatus::Maybe,
            },
            RsvpAttendee {
                id: "gWfsHvDg".into(),
                token: "d15cf90c".into(),
                email: "bar@localhost".into(),
                status: CalendarAttendeeStatus::Unanswered,
            },
        ],
        organizer: RsvpOrganizer {
            email: "foo@localhost".into(),
        },
        calendar: RsvpCalendar {
            id: "HzNtbT1J".into(),
            name: "My calendar".into(),
            color: "#273EB2".into(),
        },
        raw: Box::new(raw),
    }
}
