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
    RsvpAttendee, RsvpCache, RsvpCalendar, RsvpContacts, RsvpEvent, RsvpEventId, RsvpFetchApiError,
    RsvpIntent, RsvpKeys, RsvpOccurrence, RsvpOrganizer, RsvpProgress, RsvpRecency, RsvpRelation,
};
use proton_core_api::services::proton::AddressId;
use proton_core_api::session::{Config, Session};
use proton_core_common::test_utils::test_context::{MockApiEnv, TestContext};
use proton_crypto::crypto::{DataEncoding, KeyGeneratorAlgorithm, PGPProviderSync};
use proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::keys::{
    KeyFlag, KeyId, LocalAddressKey, LocalUserKey, UnlockedAddressKey, UnlockedAddressKeys,
};
use proton_crypto_account::salts::KeySalt;
use proton_crypto_calendar::{CalendarEventEncryptor, KeyPacket, UnlockedCalendarKey};
use proton_ical as ical;
use proton_network_monitor_service::ConnectionMonitor;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
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
    ORGANIZER:mailto:foo@pm.me
    ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION:mailto:bar@pm.me
    ATTENDEE;ROLE=OPT-PARTICIPANT:mailto:zar@pm.me
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
    ATTENDEE;CN=foo@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=245902dc:mailto:foo@pm.me
    ATTENDEE;CN=bar@pm.me;ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=d15cf90c:mailto:bar@pm.me
    ATTENDEE;CN=zar@pm.me;ROLE=OPT-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN=fdec9604:mailto:zar@pm.me
    END:VEVENT
    END:VCALENDAR
"};

const BAR_ATTENDEE_ID: &str = "gWfsHvDg";
const BAR_ATTENDEE_TOKEN: &str = "d15cf90c";

const FOO_ATTENDEE_ID: &str = "V3FdcecX";
const FOO_ATTENDEE_TOKEN: &str = "245902dc";

const ZAR_ATTENDEE_ID: &str = "m4x8IpHm";
const ZAR_ATTENDEE_TOKEN: &str = "fdec9604";

const XAR_ATTENDEE_ID: &str = "bnkeT2C8";
const XAR_ATTENDEE_TOKEN: &str = "8CDyJHVR";

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
    keys: DummyRsvpKeys,
    address_keys: UnlockedAddressKeys<P>,
    calendar_keys: RefCell<HashMap<CalendarId, UnlockedCalendarKey<P>>>,
    cache: DummyRsvpCache,
    contacts: DummyRsvpContacts,
    now: Zoned,
    connection_monitor: ConnectionMonitor,
}

async fn world() -> World<impl PGPProviderSync> {
    let ctx = TestContext::new().await;

    let connection_monitor = ConnectionMonitor::standalone();
    let sess = {
        let env = MockApiEnv::new(ctx.mock_server().uri()).with_path("/api");
        let cfg = Config::for_env(env);

        Session::builder()
            .with_config(&cfg)
            .with_connection_monitor(connection_monitor.clone())
            .build()
            .await
            .unwrap()
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

    let address_keys = UnlockedAddressKeys::from({
        // Generate address used to encrypt the calendar
        let key0 = LocalAddressKey::generate(
            &pgp,
            "bar@protonmail.com",
            KeyGeneratorAlgorithm::default(),
            KeyFlag::default(),
            false,
            &user_key,
        )
        .unwrap()
        .unlock_and_assign_key_id(&pgp, KeyId(String::new()), &user_key)
        .unwrap();

        // Generate address used to encrypt the invite (for testing
        // Proton-to-Proton invites, used in just a couple of tests)
        let key1 = LocalAddressKey::generate(
            &pgp,
            "bar@pm.me",
            KeyGeneratorAlgorithm::default(),
            KeyFlag::default(),
            true,
            &user_key,
        )
        .unwrap()
        .unlock_and_assign_key_id(&pgp, KeyId(String::new()), &user_key)
        .unwrap();

        vec![key0, key1]
    });

    let keys = {
        let keys = address_keys
            .iter()
            .map(|key| {
                pgp.private_key_export(&key.private_key, "test", DataEncoding::Armor)
                    .unwrap()
                    .as_ref()
                    .to_vec()
            })
            .collect();

        DummyRsvpKeys { keys }
    };

    World {
        ctx,
        sess,
        pgp,
        keys,
        address_keys,
        calendar_keys: RefCell::default(),
        cache: DummyRsvpCache,
        contacts: DummyRsvpContacts,
        now: "20180101T100000[UTC]".parse().unwrap(),
        connection_monitor,
    }
}

impl<P> World<P>
where
    P: PGPProviderSync,
{
    fn bootstrap(&self) -> CalendarBootstrap {
        self.bootstrap_ex(CALENDAR_ID)
    }

    fn bootstrap_ex(&self, id: &str) -> CalendarBootstrap {
        let key = self
            .calendar_keys
            .borrow_mut()
            .entry(CalendarId::from(id))
            .or_insert_with(|| UnlockedCalendarKey::generate(&self.pgp).unwrap())
            .export(&self.pgp, &self.address_keys[0])
            .unwrap();

        CalendarBootstrap {
            keys: vec![CalendarKey {
                id: id.into(),
                private_key: key.key().into(),
                flags: CalendarKeyFlags::ActiveAndPrimary,
            }],
            passphrase: CalendarPassphrase {
                member_passphrases: vec![CalendarMemberPassphrase {
                    member_id: id.into(),
                    passphrase: key.passphrase().into(),
                    signature: key.signature().into(),
                }],
            },
            members: [CalendarMember {
                id: id.into(),
                name: "My calendar".into(),
                color: "#273EB2".into(),
                address_id: "addr0".into(),
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
    calendar_id: Option<&'static str>,
    shared_event: Option<&'static str>,
    shared_event_author: Option<&'static str>,
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
            calendar_id: None,
            encryption: "calendar-key",
            shared_event: None,
            shared_event_author: None,
            attendees_event: None,
            calendar_event: None,
            attendees: Vec::new(),
        }
    }

    fn basic(self) -> Self {
        self.with_id(EVENT_ID)
            .with_calendar_id(CALENDAR_ID)
            .with_shared_event(SHARED_EVENT)
            .with_attendees_event(ATTENDEES_EVENT)
            .with_attendees(ATTENDEES())
    }

    fn with_id(mut self, id: &'static str) -> Self {
        self.id = Some(id);
        self
    }

    fn with_calendar_id(mut self, calendar_id: &'static str) -> Self {
        self.calendar_id = Some(calendar_id);
        self
    }

    fn with_shared_event(mut self, ics: &'static str) -> Self {
        self.shared_event = Some(ics);
        self
    }

    fn with_shared_event_author(mut self, author: &'static str) -> Self {
        self.shared_event_author = Some(author);
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
        let mut calendar_keys = self.world.calendar_keys.borrow_mut();

        let encryptor = match self.encryption {
            "address-key" => {
                CalendarEventEncryptor::for_address_ex(&self.world.pgp, &self.world.address_keys[1])
                    .unwrap()
            }

            "calendar-key" => {
                let calendar_id = CalendarId::from(self.calendar_id.unwrap());

                let calendar_key = calendar_keys
                    .entry(calendar_id)
                    .or_insert_with(|| UnlockedCalendarKey::generate(&self.world.pgp).unwrap());

                CalendarEventEncryptor::for_calendar_ex(
                    &self.world.pgp,
                    &self.world.address_keys[0],
                    calendar_key,
                )
                .unwrap()
            }

            _ => unreachable!(),
        };

        let shared_event = self.shared_event.unwrap();
        let shared_event_author = self.shared_event_author.unwrap_or("foo@pm.me");

        let (shared_event, _) = encryptor
            .encrypt(&self.world.pgp, shared_event.as_bytes())
            .unwrap();

        let attendees_event = self.attendees_event.map(|event| {
            encryptor
                .encrypt(&self.world.pgp, event.as_bytes())
                .unwrap()
                .0
        });

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
                author: shared_event_author.into(),
            })
            .collect();

        let attendees_events = attendees_event
            .map(|event| {
                vec![CalendarEventPayload {
                    ty: CalendarEventPayloadType::Encrypted,
                    data: event.into_base64(),
                    signature: None,
                    author: "foo@pm.me".into(),
                }]
            })
            .unwrap_or_default();

        CalendarEvent {
            shared_events: vec![CalendarEventPayload {
                ty: CalendarEventPayloadType::Encrypted,
                data: shared_event.into_base64(),
                signature: None,
                author: "foo@pm.me".into(),
            }],
            calendar_events,
            id: self.id.unwrap().into(),
            address_id: Some("addr1".into()),
            calendar_id: self.calendar_id.unwrap().into(),
            address_key_packet,
            shared_key_packet,
            attendees_events,
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

struct DummyRsvpContacts;

impl RsvpContacts for DummyRsvpContacts {
    async fn get_display_name(&self, email: &str) -> Option<String> {
        match email {
            "bar@pm.me" => Some("Bar Localhosty".into()),
            "foo@pm.me" => Some("Foo Localhosty".into()),
            "xar@pm.me" => Some("xar@pm.me".into()),
            _ => None,
        }
    }
}

struct DummyRsvpKeys {
    keys: Vec<Vec<u8>>,
}

impl DummyRsvpKeys {
    fn import_key<P>(pgp: &P, key: &[u8]) -> UnlockedAddressKeys<P>
    where
        P: PGPProviderSync,
    {
        let private_key = pgp
            .private_key_import(key, "test", DataEncoding::Armor)
            .unwrap();

        let public_key = pgp.private_key_to_public_key(&private_key).unwrap();

        UnlockedAddressKeys(vec![UnlockedAddressKey::<P> {
            id: "1234".into(),
            flags: 0_u32.into(),
            primary: true,
            is_v6: false,
            private_key,
            public_key,
        }])
    }
}

impl RsvpKeys for DummyRsvpKeys {
    type Error = io::Error;

    async fn get_address_keys<P>(
        &self,
        pgp: &P,
        id: &AddressId,
    ) -> Result<UnlockedAddressKeys<P>, Self::Error>
    where
        P: PGPProviderSync,
    {
        match id.as_str() {
            "addr0" => Ok(Self::import_key(pgp, &self.keys[0])),
            "addr1" => Ok(Self::import_key(pgp, &self.keys[1])),
            id => panic!("unexpected address: {id}"),
        }
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
            name: Some("Foo Localhosty".into()),
            reply_email: "foo@pm.me".into(),
            display_email: "foo@pm.me".into(),
        },
        attendees: vec![
            RsvpAttendee {
                id: Some(BAR_ATTENDEE_ID.into()),
                token: Some(BAR_ATTENDEE_TOKEN.into()),
                name: Some("Bar Localhosty".into()),
                email: "bar@pm.me".into(),
                status: Some(CalendarAttendeeStatus::Unanswered),
                role: ical::Role::ReqParticipant,
            },
            RsvpAttendee {
                id: Some(ZAR_ATTENDEE_ID.into()),
                token: Some(ZAR_ATTENDEE_TOKEN.into()),
                name: None,
                email: "zar@pm.me".into(),
                status: Some(CalendarAttendeeStatus::Yes),
                role: ical::Role::OptParticipant,
            },
        ],
        relation: RsvpRelation::Attendee { attendee_idx: 0 },
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

fn expected_offline_event(err: RsvpFetchApiError) -> RsvpEvent {
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
            name: Some("Foo Localhosty".into()),
            reply_email: "foo@pm.me".into(),
            display_email: "foo@pm.me".into(),
        },
        attendees: vec![
            RsvpAttendee {
                id: None,
                token: None,
                name: Some("Bar Localhosty".into()),
                email: "bar@pm.me".into(),
                status: None,
                role: ical::Role::ReqParticipant,
            },
            RsvpAttendee {
                id: None,
                token: None,
                name: None,
                email: "zar@pm.me".into(),
                status: None,
                role: ical::Role::OptParticipant,
            },
        ],
        relation: RsvpRelation::Attendee { attendee_idx: 0 },
        calendar: None,
        progress: RsvpProgress::Pending,
        recency: RsvpRecency::Unknown(err),
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
