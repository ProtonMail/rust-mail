mod rsvp {
    mod answer;
    mod fetch;
}

use indoc::indoc;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use proton_calendar_api::{
    CalendarAttendee, CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent,
    CalendarEventPayload, CalendarEventPayloadType, CalendarId, CalendarKey, CalendarKeyFlags,
    CalendarMember, CalendarMemberPassphrase, CalendarPassphrase,
};
use proton_calendar_common::{
    RsvpAttendee, RsvpCache, RsvpCalendar, RsvpEvent, RsvpEventId, RsvpIntent, RsvpOccurrence,
    RsvpOrganizer, RsvpProgress, RsvpRecency,
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
    ORGANIZER:mailto:foo@localhost
    ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@localhost
    END:VEVENT
    END:VCALENDAR
"};

const SHARED_EVENT: &str = indoc! {"
    BEGIN:VCALENDAR
    VERSION:2.0
    BEGIN:VEVENT
    UID:8maQ3qBa
    DTSTAMP:20180101T080000Z
    DTSTART:20180101T120000Z
    DTEND:20180101T133000Z
    DESCRIPTION:some description
    SUMMARY:some title
    LOCATION:some location
    END:VEVENT
    END:VCALENDAR
"};

const ATTENDEES_EVENT: &str = indoc! {"
    BEGIN:VCALENDAR
    VERSION:2.0
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
    cache: DummyRsvpCache,
    now: Zoned,
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
        cache: DummyRsvpCache,
        now: "20180101T100000[UTC]".parse().unwrap(),
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

    fn event(
        &self,
        mode: &str,
        shared_event: &str,
        attendees_event: &str,
        calendar_event: Option<&str>,
    ) -> CalendarEvent {
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

        let calendar_events = calendar_event
            .into_iter()
            .map(|data| CalendarEventPayload {
                ty: CalendarEventPayloadType::ClearText,
                data: data.into(),
                signature: None,
                author: "foo@localhost".into(),
            })
            .collect();

        CalendarEvent {
            shared_events: vec![CalendarEventPayload {
                ty: CalendarEventPayloadType::Encrypted,
                data: shared_event.into_base64(),
                signature: None,
                author: "foo@localhost".into(),
            }],
            calendar_events,
            id: "pFmwNlJp".into(),
            calendar_id: "HzNtbT1J".into(),
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
            notifications: None,
            color: Some("#aabbcc".into()),
            is_proton_proton_invite: true,
        }
    }
}

struct DummyRsvpCache;

impl RsvpCache for DummyRsvpCache {
    fn get_calendar_bootstrap<E, Fn, Fut>(
        &self,
        _: &CalendarId,
        fetch: Fn,
    ) -> impl Future<Output = Result<CalendarBootstrap, E>>
    where
        Fn: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<CalendarBootstrap, E>> + Send,
    {
        fetch()
    }
}

fn expected_event(intent: RsvpIntent, raw: CalendarEvent) -> RsvpEvent {
    RsvpEvent {
        intent,
        summary: Some("some title".into()),
        location: Some("some location".into()),
        description: Some("some description".into()),
        recurrence: None,
        occurrence: RsvpOccurrence::DateTime {
            starts_at: Zoned::new(
                Timestamp::from_second(1_514_808_000).unwrap(),
                TimeZone::UTC,
            ),
            ends_at: Zoned::new(
                Timestamp::from_second(1_514_813_400).unwrap(),
                TimeZone::UTC,
            ),
        },
        organizer: RsvpOrganizer {
            email: "foo@localhost".into(),
        },
        attendees: vec![RsvpAttendee {
            id: Some("gWfsHvDg".into()),
            token: Some("d15cf90c".into()),
            email: "bar@localhost".into(),
            status: Some(CalendarAttendeeStatus::Unanswered),
        }],
        calendar: Some(RsvpCalendar {
            id: "HzNtbT1J".into(),
            name: "My calendar".into(),
            color: "#273EB2".into(),
        }),
        progress: RsvpProgress::Pending,
        recency: RsvpRecency::Fresh,
        raw: Some(Box::new(raw)),
    }
}

fn expected_offline_event() -> RsvpEvent {
    RsvpEvent {
        intent: RsvpIntent::Invite,
        summary: Some("some title".into()),
        location: Some("some location".into()),
        description: Some("some description".into()),
        recurrence: None,
        occurrence: RsvpOccurrence::DateTime {
            starts_at: Zoned::new(
                Timestamp::from_second(1_514_808_000).unwrap(),
                TimeZone::UTC,
            ),
            ends_at: Zoned::new(
                Timestamp::from_second(1_514_813_400).unwrap(),
                TimeZone::UTC,
            ),
        },
        organizer: RsvpOrganizer {
            email: "foo@localhost".into(),
        },
        attendees: vec![RsvpAttendee {
            id: None,
            token: None,
            email: "bar@localhost".into(),
            status: None,
        }],
        calendar: None,
        progress: RsvpProgress::Pending,
        recency: RsvpRecency::Unknown,
        raw: None,
    }
}

trait RsvpEventIdExt
where
    Self: Sized,
{
    /// Creates an [`RsvpEventId`] that fakes an `invite.ics`.
    fn invite(ics: &str) -> Self;

    /// Creates an [`RsvpEventId`] that fakes a reminder.
    fn reminder(cal_id: &str, event_id: &str) -> Self;
}

impl RsvpEventIdExt for RsvpEventId {
    fn invite(ics: &str) -> Self {
        RsvpEventId::from_invite(ics.as_bytes()).unwrap()
    }

    fn reminder(cal_id: &str, event_id: &str) -> Self {
        RsvpEventId::Reminder {
            cal_id: cal_id.into(),
            event_id: event_id.into(),
        }
    }
}
