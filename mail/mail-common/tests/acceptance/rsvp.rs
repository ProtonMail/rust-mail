use indoc::formatdoc;
use jiff::Zoned;
use proton_calendar_api::{self as cal, ProtonCalendarMock};
use proton_calendar_common::{RsvpAnswer, RsvpEventId, RsvpOrganizer};
use proton_core_api::services::proton::{GetKeysAllResponse, PrivateString, UserId};
use proton_core_common::models::{Contact, ContactEmail, ModelExtension};
use proton_crypto_calendar::{CalendarEventEncryptor, KeyPacket, UnlockedCalendarKey};
use proton_crypto_inbox::attachment::{EncryptableAttachment, KeyPackets};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::prelude::{self as mail, ContentDisposition};
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ADDRESS_ID, TEST_USER_ID, message_body_test_message_simple, message_body_test_params,
    message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;
use std::str::FromStr;

const CALENDAR_ID: &str = "yXbOd5cP";
const CALENDAR_KEY_ID: &str = "BIenFMoT";
const CALENDAR_MEMBER_ID: &str = "MiMwchWT";

const EVENT_ID: &str = "PBBbBExE";
const EVENT_UID: &str = "TqUvdTrE@proton.me";

const SPONGEBOB_MAIL: &str = "spongebob@pm.me";
const SPONGEBOB_NAME: &str = "Sponge Bob";
const SPONGEBOB_ATTENDEE_ID: &str = "kdLoSTNf";
const SPONGEBOB_ATTENDEE_TOKEN: &str = "JsgBUhNM";

const TEST_MAIL: &str = "RUST_TEST+lovesinvites@proton.ch";
const TEST_ATTENDEE_ID: &str = "Rh4V1hbc";
const TEST_ATTENDEE_TOKEN: &str = "yAFY4dMB";
const TEST_ATTACHMENT_ID: &str = "EZAYcqch";

static INVITE: fn() -> String = || {
    formatdoc! {"
        BEGIN:VCALENDAR
        METHOD:REQUEST
        BEGIN:VEVENT
        UID:{EVENT_UID}
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T130000Z
        SUMMARY:face-to-face with rust-test
        LOCATION:bikini bottom
        END:VEVENT
        END:VCALENDAR
    "}
};

static SHARED_EVENT: fn() -> String = || {
    formatdoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:{EVENT_UID}
        DTSTAMP:20180101T080000Z
        DTSTART:20180101T120000Z
        DTEND:20180101T130000Z
        SUMMARY:face-to-face with rust-test
        LOCATION:bikini bottom
        END:VEVENT
        END:VCALENDAR
    "}
};

static ATTENDEES_EVENT: fn() -> String = || {
    formatdoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:{EVENT_UID}
        ATTENDEE;CN={SPONGEBOB_MAIL};ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN={SPONGEBOB_ATTENDEE_TOKEN}:mailto:{SPONGEBOB_MAIL}
        ATTENDEE;CN={TEST_MAIL};ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN={TEST_ATTENDEE_TOKEN}:mailto:{TEST_MAIL}
        END:VEVENT
        END:VCALENDAR
    "}
};

struct InviteIcs(String);

impl InviteIcs {
    fn new() -> Self {
        Self(INVITE())
    }
}

impl EncryptableAttachment for InviteIcs {
    fn attachment_data(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[tokio::test]
async fn fetch_and_answer() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.setup_user(message_body_test_params()).await;

    let now = Zoned::from_str("20180101T080000[UTC]").unwrap();
    let user_ctx = ctx.mail_user_context().await;
    let user_address_id = TEST_USER_ADDRESS_ID.into();

    ctx.core_context().clock().pretend(now.clone());

    let mut db = user_ctx.user_stash().connection().await.unwrap();
    let db2 = user_ctx.user_stash().connection().await.unwrap();

    // ---

    let pgp = new_pgp_provider();

    let address_keys = ctx
        .mail_user_context()
        .await
        .unlocked_address_keys(&pgp, &db, &user_address_id)
        .await
        .unwrap();

    let calendar_key = UnlockedCalendarKey::generate(&pgp).unwrap();

    // ---

    let mut sb = Contact {
        remote_id: Some("100".into()),
        contact_emails: vec![ContactEmail {
            remote_id: Some("1000".into()),
            canonical_email: SPONGEBOB_MAIL.into(),
            name: SPONGEBOB_NAME.into(),
            ..ContactEmail::test_default()
        }],
        ..Contact::test_default()
    };

    db.tx(async |tx| sb.save(tx).await).await.unwrap();

    db.tx(async |tx| sb.contact_emails[0].save(tx).await)
        .await
        .unwrap();

    // ---
    // Step 1: Fetch the message.

    let ics_fixture = {
        let key = address_keys.primary_for_mail().unwrap();

        InviteIcs::new()
            .attachment_encrypt_and_sign(&pgp, &key)
            .unwrap()
    };

    let msg_fixture = {
        let mut msg = message_body_test_message_simple();

        msg.metadata.subject = "Invitation for an event".into();

        msg.metadata.to_list = vec![mail::MessageRecipient {
            address: TEST_MAIL.into(),
            is_proton: true,
            name: PrivateString::default(),
            group: None,
        }];

        msg.metadata.attachments_metadata = vec![mail::AttachmentMetadata {
            id: mail::AttachmentId::from(TEST_ATTACHMENT_ID),
            size: 0,
            name: "attachment.txt".into(),
            mime_type: "text/calendar".into(),
            disposition: mail::Disposition::Attachment,
        }];

        msg.body.attachments = vec![mail::MessageAttachment {
            id: mail::AttachmentId::from(TEST_ATTACHMENT_ID),
            disposition: mail::Disposition::Attachment,
            enc_signature: None,
            headers: mail::MessageAttachmentHeaders {
                content_disposition: ContentDisposition::One("attachment".into()),
                content_id: None,
                content_transfer_encoding: None,
                image_height: None,
                image_width: None,
            },
            key_packets: KeyPackets::new_from_bytes(&ics_fixture.metadata.key_packets),
            mime_type: "text/calendar".into(),
            name: "invite.ics".into(),
            signature: None,
            size: 0,
        }];

        msg.body
            .parsed_headers
            .insert("X-Original-To".into(), TEST_MAIL.into());

        msg.body
            .parsed_headers
            .insert("X-Pm-Calendar-Eventuid".into(), EVENT_UID.into());

        msg
    };

    ctx.mock_get_message(&msg_fixture.metadata.id, msg_fixture.clone())
        .await;

    ctx.mock_get_attachment_data(TEST_ATTACHMENT_ID.into(), ics_fixture.data, 1)
        .await;

    // ---

    let (mut msg, _, _) = Message::from_api_data(msg_fixture.clone(), &db)
        .await
        .unwrap();

    db.tx(async |tx| msg.save(tx).await).await.unwrap();

    let msg_body = msg.fetch_message_body(&user_ctx, &mut db).await.unwrap();

    // ---
    // Step 2: Find RSVP.

    let rsvp = msg_body.identify_rsvp(&user_ctx).await.unwrap().unwrap();

    let RsvpEventId::Invite { uid, rid, .. } = &*rsvp else {
        panic!("expected an invite");
    };

    assert_eq!("TqUvdTrE@proton.me", uid.as_str());
    assert!(rid.is_none());

    // ---
    // Step 3: Load RSVP details from the calendar.
    //
    // This requires a bit of code, because we need to pretend that both the
    // calendar and the event exist, and that both are encrypted.
    //
    // That's because even though an event can contain unencrypted bits - see
    // `event.shared_events[].ty` - we need to provide either of the key packets
    // (`address_key_packet` or `shared_key_packet`), they can't be both empty.
    //
    // (they can't be spurious either, they have to contain a god-loving genuine
    // session key.)

    let calendar = {
        let key = calendar_key.export(&pgp, &address_keys[0]).unwrap();

        cal::CalendarBootstrap {
            keys: vec![cal::CalendarKey {
                id: CALENDAR_KEY_ID.into(),
                private_key: key.key().into(),
                flags: cal::CalendarKeyFlags::ActiveAndPrimary,
            }],
            passphrase: cal::CalendarPassphrase {
                member_passphrases: vec![cal::CalendarMemberPassphrase {
                    member_id: CALENDAR_MEMBER_ID.into(),
                    passphrase: key.passphrase().into(),
                    signature: key.signature().into(),
                }],
            },
            members: [cal::CalendarMember {
                id: CALENDAR_MEMBER_ID.into(),
                name: "My calendar".into(),
                color: "#273EB2".into(),
                address_id: msg_fixture.metadata.address_id.clone(),
            }],
        }
    };

    let event = {
        let encryptor = CalendarEventEncryptor::for_address(&pgp, &address_keys).unwrap();
        let (shared_event, _) = encryptor.encrypt(&pgp, SHARED_EVENT().as_bytes()).unwrap();

        let (attendees_event, _) = encryptor
            .encrypt(&pgp, ATTENDEES_EVENT().as_bytes())
            .unwrap();

        let key_packets = encryptor.finish(&pgp).unwrap();
        let address_key_packet = key_packets.address_key_packet.map(KeyPacket::into_base64);
        let shared_key_packet = key_packets.shared_key_packet.map(KeyPacket::into_base64);

        cal::CalendarEvent {
            shared_events: vec![cal::CalendarEventPayload {
                ty: cal::CalendarEventPayloadType::Encrypted,
                data: shared_event.into_base64(),
                signature: None,
                author: SPONGEBOB_MAIL.into(),
            }],
            calendar_events: vec![],
            id: EVENT_ID.into(),
            address_id: None,
            calendar_id: CALENDAR_ID.into(),
            address_key_packet,
            shared_key_packet,
            attendees_events: vec![cal::CalendarEventPayload {
                ty: cal::CalendarEventPayloadType::Encrypted,
                data: attendees_event.into_base64(),
                signature: None,
                author: SPONGEBOB_MAIL.into(),
            }],
            attendees: vec![
                cal::CalendarAttendee {
                    id: SPONGEBOB_ATTENDEE_ID.into(),
                    token: SPONGEBOB_ATTENDEE_TOKEN.into(),
                    status: cal::CalendarAttendeeStatus::Yes,
                },
                cal::CalendarAttendee {
                    id: TEST_ATTENDEE_ID.into(),
                    token: TEST_ATTENDEE_TOKEN.into(),
                    status: cal::CalendarAttendeeStatus::Unanswered,
                },
            ],
            notifications: None,
            color: None,
            is_proton_proton_invite: true,
        }
    };

    ctx.mock_web_server
        .mock_get_calendar_bootstrap(CALENDAR_ID, calendar)
        .await;

    ctx.mock_web_server
        .mock_find_calendar_events(EVENT_UID, None, vec![event])
        .await;

    let mut rsvp = rsvp.fetch(&user_ctx, &mut db).await.unwrap().unwrap();

    assert_eq!(
        RsvpOrganizer {
            name: Some(SPONGEBOB_NAME.into()),
            reply_email: SPONGEBOB_MAIL.into(),
            display_email: SPONGEBOB_MAIL.into(),
        },
        rsvp.organizer,
    );

    assert_eq!(Some("face-to-face with rust-test"), rsvp.summary.as_deref());

    // ---
    // Step 4: Answer RSVP.
    //
    // This performs a couple of things:
    //
    // - it upgrades event's encryption from `address_key_packet` into
    //   `shared_key_packet` (see the calendar crate for details),
    // - it updates our attendee-status in the calendar,
    // - it updates our notifications,
    // - and, finally, it sends an email to the organizer.

    ctx.mock_web_server
        .mock_upgrade_calendar_event_invite(CALENDAR_ID, EVENT_ID)
        .await;

    ctx.mock_web_server
        .mock_update_calendar_event_attendee_status(
            CALENDAR_ID,
            EVENT_ID,
            TEST_ATTENDEE_ID,
            cal::CalendarAttendeeStatus::Yes,
            &now,
        )
        .await;

    ctx.mock_web_server
        .mock_update_calendar_event_personal_part(
            CALENDAR_ID,
            EVENT_ID,
            None,
            cal::CalendarNotificationsUpdate::SetToDefault,
        )
        .await;

    ctx.core_test_context()
        .mock_get_keys_all(
            SPONGEBOB_MAIL,
            GetKeysAllResponse {
                address_keys: Default::default(),
                catch_all_keys: None,
                is_proton: false,
                proton_mx: false,
                unverified_keys: None,
                warnings: vec![],
            },
        )
        .await;

    ctx.mock_send_direct(
        "Re: Invitation for an event",
        "rust_test+lovesinvites@proton.ch",
        SPONGEBOB_MAIL,
        &["invite.ics"],
        Some(
            "blkMQzCHplN2H_FNJ2GdMtRkmr3f9v_cFma64_Cmi8IPw3wx_lK-0ZEqA8cBfIf0Pe\
             VbY2P7oVQVwPup-h0syg==",
        ),
        mail::PostSendDirectMessageResponse {
            sent: mail::Message {
                metadata: mail::MessageMetadata {
                    num_attachments: 1,
                    ..msg_fixture.metadata.clone()
                },
                body: mail::MessageBody {
                    attachments: vec![mail::MessageAttachment {
                        id: mail::AttachmentId::new("cHIs3FzX".into()),
                        disposition: mail::Disposition::Attachment,
                        enc_signature: None,
                        headers: mail::MessageAttachmentHeaders {
                            content_disposition: ContentDisposition::One("attachment".into()),
                            content_id: None,
                            content_transfer_encoding: None,
                            image_height: None,
                            image_width: None,
                        },
                        key_packets: KeyPackets::new_from_bytes(&[]),
                        mime_type: "text/calendar".into(),
                        name: "invite.ics".into(),
                        signature: None,
                        size: 123,
                    }],
                    ..msg_fixture.body.clone()
                },
            },
        },
    )
    .await;

    ctx.catch_all().await;

    assert!(
        !rsvp
            .attendees
            .iter()
            .all(|att| att.status == Some(cal::CalendarAttendeeStatus::Yes))
    );

    rsvp.answer(&user_ctx, &mut db, &db2, RsvpAnswer::Yes)
        .await
        .unwrap();

    assert!(
        rsvp.attendees
            .iter()
            .all(|att| att.status == Some(cal::CalendarAttendeeStatus::Yes))
    );

    msg.reload(&db).await.unwrap();

    assert_eq!(1, msg.attachments_metadata.len());
    assert_eq!("invite.ics", msg.attachments_metadata[0].filename);
    assert_eq!(123, msg.attachments_metadata[0].size);
}
