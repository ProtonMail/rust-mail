use indoc::formatdoc;
use jiff::Zoned;
use proton_calendar_api::{self as cal, ProtonCalendarMock};
use proton_calendar_common::{RsvpAnswerStatus, RsvpEventId};
use proton_core_api::services::proton::{GetKeysAllResponse, LabelId, UserId};
use proton_core_common::models::ModelExtension;
use proton_crypto_calendar::{CalendarEventEncryptor, KeyPacket, UnlockedCalendarKey};
use proton_crypto_inbox::attachment::KeyPackets;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::prelude as mail;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
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
const SPONGEBOB_ATT_ID: &str = "kdLoSTNf";
const SPONGEBOB_ATT_TOKEN: &str = "JsgBUhNM";

const TEST_MAIL: &str = "rust_test@proton.ch";
const TEST_ATT_ID: &str = "Rh4V1hbc";
const TEST_ATT_TOKEN: &str = "yAFY4dMB";

static SHARED_EVENT: fn() -> String = || {
    formatdoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        BEGIN:VEVENT
        UID:{EVENT_UID}
        DTSTAMP:20180101T120000Z
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
        ATTENDEE;CN={SPONGEBOB_MAIL};ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN={SPONGEBOB_ATT_TOKEN}:mailto:{SPONGEBOB_MAIL}
        ATTENDEE;CN={TEST_MAIL};ROLE=REQ-PARTICIPANT;RSVP=TRUE;X-PM-TOKEN={TEST_ATT_TOKEN}:mailto:{TEST_MAIL}
        END:VEVENT
        END:VCALENDAR
    "}
};

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

    let mut tx = user_ctx.user_stash().connection();

    // ---

    let pgp = new_pgp_provider();

    let address_keys = ctx
        .mail_user_context()
        .await
        .unlocked_address_keys(&pgp, &tx, &user_address_id)
        .await
        .unwrap();

    let calendar_key = UnlockedCalendarKey::generate(&pgp).unwrap();

    // ---
    // Step 1: Fetch the message.

    let message = {
        let mut message = message_body_test_message_simple();

        message.metadata.subject = "Invitation for an event".into();

        message.metadata.to_list = vec![mail::MessageRecipient {
            address: TEST_MAIL.into(),
            is_proton: true,
            name: String::default(),
            group: None,
        }];

        message
            .body
            .parsed_headers
            .insert("X-Pm-Calendar-Eventuid".into(), EVENT_UID.into());

        message
    };

    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    ctx.mock_get_messages(vec![message.metadata.clone()]).await;

    Mailbox::with_remote_id(&tx, LabelId::inbox())
        .await
        .unwrap()
        .sync(&mut tx, user_ctx.api(), 1)
        .await
        .unwrap();

    let mut msg = Message::load(1.into(), &tx).await.unwrap().unwrap();
    let msg_body = msg.fetch_message_body(&user_ctx, &mut tx).await.unwrap();

    // ---
    // Step 2: Find RSVP.
    //
    // In this case we're identifying it via headers (X-Pm-Calendar-Eventuid),
    // but we could've generated an `invite.ics` attachment instead as well
    // (it's just more difficult and the outcome is the same, so why bother).

    let rsvp = msg_body.identify_rsvp(&user_ctx).await.unwrap().unwrap();

    assert_eq!(RsvpEventId::indirect(EVENT_UID, None), rsvp);

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
            calendar_id: CALENDAR_ID.into(),
            start_time: 1_514_804_400,
            end_time: 1_514_806_200,
            full_day: false,
            recurrence_id: None,
            address_key_packet,
            shared_key_packet,
            attendees_events: [cal::CalendarEventPayload {
                ty: cal::CalendarEventPayloadType::Encrypted,
                data: attendees_event.into_base64(),
                signature: None,
                author: SPONGEBOB_MAIL.into(),
            }],
            attendees: vec![
                cal::CalendarAttendee {
                    id: SPONGEBOB_ATT_ID.into(),
                    token: SPONGEBOB_ATT_TOKEN.into(),
                    status: cal::CalendarAttendeeStatus::Yes,
                },
                cal::CalendarAttendee {
                    id: TEST_ATT_ID.into(),
                    token: TEST_ATT_TOKEN.into(),
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
        .mock_find_calendar_events(EVENT_UID, None, Some(event))
        .await;

    let mut rsvp = msg
        .fetch_rsvp(&user_ctx, &mut tx, &rsvp)
        .await
        .unwrap()
        .unwrap();

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
            TEST_ATT_ID,
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
        TEST_MAIL,
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
                    ..message.metadata.clone()
                },
                body: mail::MessageBody {
                    attachments: vec![mail::MessageAttachment {
                        id: mail::AttachmentId::new("cHIs3FzX".into()),
                        disposition: mail::Disposition::Attachment,
                        enc_signature: None,
                        headers: mail::MessageAttachmentHeaders {
                            content_disposition: "attachment".into(),
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
                    ..message.body.clone()
                },
            },
        },
    )
    .await;

    assert!(
        !rsvp
            .attendees
            .iter()
            .all(|att| att.status == cal::CalendarAttendeeStatus::Yes)
    );

    rsvp.answer(&user_ctx, &mut tx, RsvpAnswerStatus::Yes)
        .await
        .unwrap();

    assert!(
        rsvp.attendees
            .iter()
            .all(|att| att.status == cal::CalendarAttendeeStatus::Yes)
    );

    msg.reload(&tx).await.unwrap();

    assert_eq!(1, msg.attachments_metadata.len());
    assert_eq!("invite.ics", msg.attachments_metadata[0].filename);
    assert_eq!(123, msg.attachments_metadata[0].size);
}
