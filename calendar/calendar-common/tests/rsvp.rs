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
use proton_ical as ical;
use std::sync::Arc;

const EVENT_ID: &str = "pFmwNlJp";
const EVENT_UID: &str = "8maQ3qBa";
const CALENDAR_ID: &str = "HzNtbT1J";

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
    ATTENDEE;ROLE=OPT-PARTICIPANT:mailto:zar@localhost
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
    UID:8maQ3qBa
    ATTENDEE;CN=foo@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@localhost
    ATTENDEE;CN=bar@localhost;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@localhost
    ATTENDEE;CN=zar@localhost;ROLE=OPT-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=fdec9604:mailto:zar@localhost
    END:VEVENT
    END:VCALENDAR
"};

const BAR_ATTENDEE_ID: &str = "gWfsHvDg";
const BAR_ATTENDEE_TOKEN: &str = "d15cf90c";

const FOO_ATTENDEE_ID: &str = "V3FdcecX";
const FOO_ATTENDEE_TOKEN: &str = "245902dc";

const ZAR_ATTENDEE_ID: &str = "m4x8IpHm";
const ZAR_ATTENDEE_TOKEN: &str = "fdec9604";

const ATTENDEES: fn() -> Vec<CalendarAttendee> = || {
    vec![
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
        CalendarAttendee {
            id: ZAR_ATTENDEE_ID.into(),
            token: ZAR_ATTENDEE_TOKEN.into(),
            status: CalendarAttendeeStatus::Yes,
        },
    ]
};

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

    fn event(&self, f: impl FnOnce(EventBuilder<P>) -> EventBuilder<P>) -> CalendarEvent {
        f(EventBuilder::new(self)).build()
    }
}

struct EventBuilder<'a, P>
where
    P: PGPProviderSync,
{
    world: &'a World<P>,
    encryption: &'static str,
    id: Option<&'static str>,
    shared_event: Option<&'static str>,
    attendees_event: Option<&'static str>,
    calendar_event: Option<&'static str>,
    attendees: Vec<CalendarAttendee>,
}

impl<'a, P> EventBuilder<'a, P>
where
    P: PGPProviderSync,
{
    fn new(world: &'a World<P>) -> Self {
        Self {
            world,
            id: None,
            encryption: "calendar-key",
            shared_event: None,
            attendees_event: None,
            calendar_event: None,
            attendees: Vec::new(),
        }
    }

    fn basic(self) -> Self {
        self.with_id(EVENT_ID)
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
            .with_attendees(ATTENDEES())
    }

    fn with_id(mut self, id: &'static str) -> Self {
        self.id = Some(id);
        self
    }

    fn with_shared_event(mut self, ics: &'static str) -> Self {
        self.shared_event = Some(ics);
        self
    }

    fn with_attendees_event(mut self, ics: &'static str) -> Self {
        self.attendees_event = Some(ics);
        self
    }

    fn with_calendar_event(mut self, ics: &'static str) -> Self {
        self.calendar_event = Some(ics);
        self
    }

    fn with_attendee(mut self, id: &str, token: &str, status: CalendarAttendeeStatus) -> Self {
        self.attendees.push(CalendarAttendee {
            id: id.into(),
            token: token.into(),
            status,
        });

        self
    }

    fn with_attendees(mut self, atts: Vec<CalendarAttendee>) -> Self {
        self.attendees.extend(atts);
        self
    }

    fn using_address_key(mut self) -> Self {
        self.encryption = "address-key";
        self
    }

    fn build(self) -> CalendarEvent {
        let encryptor = match self.encryption {
            "address-key" => {
                CalendarEventEncryptor::for_address(&self.world.pgp, &self.world.address_keys)
                    .unwrap()
            }

            "calendar-key" => CalendarEventEncryptor::for_calendar(
                &self.world.pgp,
                &self.world.address_keys,
                &self.world.calendar_key,
            )
            .unwrap(),

            _ => unreachable!(),
        };

        let shared_event = self.shared_event.unwrap();
        let attendees_event = self.attendees_event.unwrap();

        let (shared_event, _) = encryptor
            .encrypt(&self.world.pgp, shared_event.as_bytes())
            .unwrap();

        let (attendees_event, _) = encryptor
            .encrypt(&self.world.pgp, attendees_event.as_bytes())
            .unwrap();

        let key_packets = encryptor.finish(&self.world.pgp).unwrap();
        let address_key_packet = key_packets.address_key_packet.map(KeyPacket::into_base64);
        let shared_key_packet = key_packets.shared_key_packet.map(KeyPacket::into_base64);

        let calendar_events = self
            .calendar_event
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
            id: self.id.unwrap().into(),
            calendar_id: CALENDAR_ID.into(),
            address_key_packet,
            shared_key_packet,
            attendees_events: [CalendarEventPayload {
                ty: CalendarEventPayloadType::Encrypted,
                data: attendees_event.into_base64(),
                signature: None,
                author: "foo@localhost".into(),
            }],
            attendees: self.attendees,
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
        attendees: vec![
            RsvpAttendee {
                id: Some(BAR_ATTENDEE_ID.into()),
                token: Some(BAR_ATTENDEE_TOKEN.into()),
                email: "bar@localhost".into(),
                status: Some(CalendarAttendeeStatus::Unanswered),
                role: ical::Role::ReqParticipant,
            },
            RsvpAttendee {
                id: Some(ZAR_ATTENDEE_ID.into()),
                token: Some(ZAR_ATTENDEE_TOKEN.into()),
                email: "zar@localhost".into(),
                status: Some(CalendarAttendeeStatus::Yes),
                role: ical::Role::OptParticipant,
            },
        ],
        user_attendee_idx: 0,
        calendar: Some(RsvpCalendar {
            id: "HzNtbT1J".into(),
            name: "My calendar".into(),
            color: "#273EB2".into(),
        }),
        progress: RsvpProgress::Pending,
        recency: RsvpRecency::Fresh,
        raw: Some(Box::new(raw)),
        children: Vec::new(),
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
        attendees: vec![
            RsvpAttendee {
                id: None,
                token: None,
                email: "bar@localhost".into(),
                status: None,
                role: ical::Role::ReqParticipant,
            },
            RsvpAttendee {
                id: None,
                token: None,
                email: "zar@localhost".into(),
                status: None,
                role: ical::Role::OptParticipant,
            },
        ],
        user_attendee_idx: 0,
        calendar: None,
        progress: RsvpProgress::Pending,
        recency: RsvpRecency::Unknown,
        raw: None,
        children: Vec::new(),
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
