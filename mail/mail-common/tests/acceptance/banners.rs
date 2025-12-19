use proton_core_api::services::proton::LabelId;
use proton_core_common::models::Address;
use proton_core_common::models::ModelExtension as _;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::IncomingDefault as ApiIncomingDefault;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_mail_common::datatypes::MessageFlags;
use proton_mail_common::datatypes::ParsedHeaders;
use proton_mail_common::datatypes::SystemLabelId as _;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::decrypted_message::DecryptedMessageBody;
use proton_mail_common::models::Conversation;
use proton_mail_common::models::IncomingDefault;
use proton_mail_common::models::IncomingDefaultLocation;
use proton_mail_common::models::MailSettings;
use proton_mail_common::models::MessageBody;
use proton_mail_common::models::MessageBodyMetadata;
use proton_mail_common::models::MessageMimeType;

use proton_mail_common::test_utils::init::Params;
use stash::orm::Model;
use stash::stash::Tether;
use test_case::test_case;

use proton_mail_common::models::Message;

use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::test_utils::test_context::MailUserContextTestExtension;
use stash::stash::StashError;
use velcro::hash_map;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::{method, path};

fn default_api_incoming_default() -> ApiIncomingDefault {
    ApiIncomingDefault {
        email: None,
        location: ApiIncomingDefaultLocation::Blocked,
        action: None,
        id: "".into(),
        domain: None,
    }
}

#[tokio::test]
async fn banners() {
    let test_ctx = MailTestContext::new().await;
    let mut params = Params::default_basic();

    let mut addr: Address = params.addresses.pop().unwrap().into();
    let mut conv: Conversation = params.conversations.pop().unwrap().into();

    test_ctx.setup_user(params.clone()).await;

    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .named("Mock pings")
        .mount(test_ctx.mock_server())
        .await;

    let message_id = MessageId::from("normal");

    test_ctx
        .mock_label_messages(&LabelId::spam(), vec!["normal".into()])
        .await;

    test_ctx.mock_report_phishing().await;
    test_ctx.mock_delete_incoming_default().await;

    test_ctx
        .mock_post_incoming_default(ApiIncomingDefault {
            email: Some("normal@email".into()),
            ..default_api_incoming_default()
        })
        .await;

    test_ctx.mock_put_message_ham(&"spam".into()).await;
    test_ctx.mock_put_message_ham(&"phishing".into()).await;
    test_ctx.mock_put_message_ham(&"normal".into()).await;
    test_ctx
        .mock_label_messages(&LabelId::inbox(), vec!["spam".into(), "phishing".into()])
        .await;
    test_ctx
        .mock_label_messages(&LabelId::inbox(), vec!["normal".into()])
        .await;

    let ctx = test_ctx.mail_user_context().await;
    test_ctx.initialize_uninitialized_ctx(&ctx).await;

    let tether = &mut ctx.user_stash().connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            conv.save(tx).await.unwrap();
            addr.save(tx).await.unwrap();
            let mut incoming_default = IncomingDefault {
                email: Some("blocked@email".into()),
                location: IncomingDefaultLocation::Blocked,
                remote_id: Some("123".into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();

            let mut incoming_default = IncomingDefault {
                domain: Some("blocked.com".into()),
                location: IncomingDefaultLocation::Blocked,
                remote_id: Some("456".into()),
                local_id: None,
                email: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let mut msg_normal = Message {
        local_conversation_id: conv.local_id,
        label_ids: vec![LabelId::inbox()],
        remote_conversation_id: conv.remote_id.clone(),
        local_address_id: addr.id(),
        remote_address_id: addr.remote_id.clone().unwrap(),
        remote_id: Some(message_id),
        sender: proton_mail_common::datatypes::MessageSender {
            address: "normal@email".into(),
            ..Default::default()
        },
        ..Message::test_default()
    };

    let mut msg_phishing = Message {
        flags: MessageFlags::PHISHING_AUTO,
        label_ids: vec![LabelId::spam()],
        remote_id: Some("phishing".into()),
        ..msg_normal.clone()
    };

    let mut msg_spam = Message {
        flags: MessageFlags::SPAM_MANUAL,
        label_ids: vec![LabelId::spam()],
        remote_id: Some("spam".into()),
        ..msg_normal.clone()
    };

    let msg_sus = Message {
        flags: MessageFlags::FLAG_SUSPICIOUS,
        label_ids: vec![LabelId::spam()],
        remote_id: Some("sus".into()),
        ..msg_normal.clone()
    };

    let msg_expiry = Message {
        expiration_time: 42.into(),
        remote_id: Some("expiry".into()),
        ..msg_normal.clone()
    };

    let msg_blocked = Message {
        remote_id: Some("blocked".into()),
        sender: proton_mail_common::datatypes::MessageSender {
            address: "blocked@email".into(),
            ..Default::default()
        },
        ..msg_normal.clone()
    };

    let msg_blocked_domain = Message {
        remote_id: Some("blocked_domain".into()),
        sender: proton_mail_common::datatypes::MessageSender {
            address: "dave@blocked.com".into(),
            ..Default::default()
        },
        ..msg_normal.clone()
    };

    let scheduled_time = 123456_u64.into();

    let msg_schedule_send = Message {
        label_ids: vec![LabelId::all_scheduled()],
        time: scheduled_time,
        ..msg_normal.clone()
    };

    let snooze_time = 123456_u64.into();

    let msg_snoozed = Message {
        snooze_time,
        label_ids: vec![LabelId::snoozed()],
        ..msg_normal.clone()
    };

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_normal.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::Expiry {
            timestamp: 42.into()
        }],
        msg_expiry.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::BlockedSender],
        msg_blocked.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::BlockedSender],
        msg_blocked_domain.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::ScheduledSend {
            timestamp: scheduled_time
        }],
        msg_schedule_send.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::PhishingAttempt { auto: true }],
        msg_phishing.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::Spam { auto: false }],
        msg_spam.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::PhishingAttempt { auto: true }],
        msg_sus.get_banners(tether).await,
    );

    assert_eq!(
        vec![MessageBanner::Snoozed {
            timestamp: snooze_time
        }],
        msg_snoozed.get_banners(tether).await,
    );

    tether
        .tx::<_, _, StashError>(async |tx| {
            msg_normal.save(tx).await.unwrap();
            msg_normal.reload(tx).await.unwrap();
            msg_spam.save(tx).await.unwrap();
            msg_spam.reload(tx).await.unwrap();
            msg_phishing.save(tx).await.unwrap();
            msg_phishing.reload(tx).await.unwrap();

            MessageBodyMetadata {
                local_message_id: msg_normal.local_id,
                remote_message_id: msg_normal.remote_id.clone(),
                ..Default::default()
            }
            .save(tx)
            .await
            .unwrap();

            MessageBody::html("im a nigerian prince, click this link")
                .store(msg_normal.id(), tx)
                .await
                .unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Let's block, unblock and report phishing to see how labels change

    IncomingDefault::action_unblock(ctx.action_queue(), "blocked@email".into())
        .await
        .unwrap();

    Message::action_ham(ctx.action_queue(), vec![msg_spam.id(), msg_phishing.id()])
        .await
        .unwrap();

    Message::action_report_phishing(ctx.action_queue(), msg_normal.id(), tether)
        .await
        .unwrap();

    ctx.execute_all_actions().await.unwrap();
    // assert_eq!(3, ctx.execute_all_actions().await.unwrap());

    msg_normal.reload(tether).await.unwrap();
    msg_spam.reload(tether).await.unwrap();
    msg_phishing.reload(tether).await.unwrap();

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_spam.get_banners(tether).await
    );

    assert_eq!(
        vec![MessageBanner::PhishingAttempt { auto: false }],
        msg_normal.get_banners(tether).await
    );

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_blocked.get_banners(tether).await
    );

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_phishing.get_banners(tether).await
    );

    // Let's make sure that action_report_phishing gets reverted and that blocking works
    IncomingDefault::action_block(ctx.action_queue(), "normal@email".into())
        .await
        .unwrap();

    Message::action_ham(ctx.action_queue(), vec![msg_normal.id()])
        .await
        .unwrap();

    msg_normal.reload(tether).await.unwrap();

    assert_eq!(
        vec![MessageBanner::BlockedSender],
        msg_normal.get_banners(tether).await
    );

    ctx.execute_all_actions().await.unwrap();
}

#[test_case(
    LabelId::inbox(),
    MessageFlags::PHISHING_AUTO,
    None;
    "Messages outside of spam don't have the flags"
)]
#[test_case(
    LabelId::spam(),
    MessageFlags::SPAM_AUTO | MessageFlags::PHISHING_MANUAL,
    Some(MessageBanner::PhishingAttempt {auto:false});
    "Phishing has precedence over spam auto"
)]
#[test_case(
    LabelId::spam(),
    MessageFlags::SPAM_MANUAL | MessageFlags::PHISHING_AUTO,
    Some(MessageBanner::Spam {auto:false});
    "Phishing doesn't take precedence over spam if the user has moved manually to spam"
)]
#[test_case(
    LabelId::spam(),
    MessageFlags::SPAM_AUTO,
    Some(MessageBanner::Spam {auto:true});
    "Spam auto gets shown"
)]
#[test_case(
    LabelId::spam(),
    MessageFlags::empty(),
    Some(MessageBanner::Spam {auto:true});
    "No flags in spam still are auto spam"
)]
#[tokio::test]
async fn spam_banners(label: LabelId, flags: MessageFlags, res: Option<MessageBanner>) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let message = Message {
        label_ids: vec![label],
        flags,
        ..Message::test_default()
    };

    let banners = message.get_banners(&tether).await;
    let banner = banners.first();
    assert_eq!(banner, res.as_ref());
}

#[tokio::test]
async fn autodelete_and_expiry() {
    async fn update_setting(days: Option<u32>, tether: &mut Tether) {
        tether
            .tx(async |tx| {
                let mut settings = MailSettings {
                    auto_delete_spam_and_trash_days: days,
                    ..Default::default()
                };
                settings.save(tx).await
            })
            .await
            .unwrap();
    }

    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // TRASH

    let mut message = Message {
        label_ids: vec![LabelId::trash()],
        expiration_time: 2000000000.into(),
        ..Message::test_default()
    };
    update_setting(Some(30), &mut tether).await;

    let banner = message.get_banners(&tether).await[0];
    if !matches!(banner, MessageBanner::AutoDelete { timestamp: _ }) {
        panic!("Expected autodelete")
    }

    update_setting(None, &mut tether).await;
    let banner = message.get_banners(&tether).await[0];
    assert_eq!(
        banner,
        MessageBanner::Expiry {
            timestamp: message.expiration_time
        },
        "If setting is disabled, we show expiry"
    );

    update_setting(Some(0), &mut tether).await;
    let banner = message.get_banners(&tether).await[0];
    assert_eq!(
        banner,
        MessageBanner::Expiry {
            timestamp: message.expiration_time
        },
        "If setting is disabled, we show expiry"
    );

    message.expiration_time = 0.into();
    update_setting(Some(30), &mut tether).await;
    assert_eq!(
        Vec::<MessageBanner>::new(),
        message.get_banners(&tether).await,
        "No expiration time = no flags"
    );

    update_setting(None, &mut tether).await;
    assert_eq!(
        Vec::<MessageBanner>::new(),
        message.get_banners(&tether).await,
        "No expiration time = no flags"
    );

    // INBOX

    message.label_ids = vec![LabelId::inbox()];
    assert_eq!(
        Vec::<MessageBanner>::new(),
        message.get_banners(&tether).await,
        "no expiration time = no flags"
    );

    message.expiration_time = 2000000000.into();
    let banner = message.get_banners(&tether).await[0];
    assert_eq!(
        banner,
        MessageBanner::Expiry {
            timestamp: message.expiration_time
        }
    );

    // SPAM

    message.label_ids = vec![LabelId::spam()];

    let banners = message.get_banners(&tether).await;
    assert_eq!(
        (
            MessageBanner::Spam { auto: true },
            MessageBanner::Expiry {
                timestamp: message.expiration_time
            },
        ),
        (banners[0], banners[1]),
    );
    update_setting(Some(30), &mut tether).await;

    let banners = message.get_banners(&tether).await;
    assert_eq!(
        vec![
            MessageBanner::Spam { auto: true },
            MessageBanner::AutoDelete {
                timestamp: message.expiration_time
            },
        ],
        banners
    );

    message.expiration_time = 0.into();
    let banners = message.get_banners(&tether).await;
    assert_eq!(banners, vec![MessageBanner::Spam { auto: true }]);
}

#[tokio::test]
async fn banners_unsubscribe() {
    let test_ctx = MailTestContext::new().await;
    let mut params = Params::default_basic();

    test_ctx.setup_user(params.clone()).await;
    let ctx = test_ctx.mail_user_context().await;
    let tether = &mut ctx.user_stash().connection().await.unwrap();

    Mock::given(method("PUT"))
        .and(path("/api/mail/v4/messages/mark/unsubscribed"))
        .respond_with(ResponseTemplate::new(200))
        .named("mock mark unsubscribe")
        .expect(1)
        .mount(test_ctx.mock_server())
        .await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/subscribe"))
        .respond_with(ResponseTemplate::new(200))
        .named("mock unsubscribe")
        .expect(1)
        .mount(&mock_server)
        .await;

    // --

    let path = mock_server.uri() + "/subscribe";

    let headers = hash_map! {
        "List-Unsubscribe".into(): path.into()
    };

    let mut addr: Address = params.addresses.pop().unwrap().into();
    let mut conv: Conversation = params.conversations.pop().unwrap().into();

    let mut msg = Message {
        label_ids: vec![LabelId::inbox()],
        remote_conversation_id: conv.remote_id.clone(),
        remote_address_id: addr.remote_id.clone().unwrap(),
        remote_id: Some("".into()),
        sender: proton_mail_common::datatypes::MessageSender {
            address: "normal@email".into(),
            ..Default::default()
        },
        ..Message::test_default()
    };
    tether
        .tx::<_, _, StashError>(async |tx| {
            addr.save(tx).await?;
            conv.save(tx).await?;
            msg.local_conversation_id = conv.local_id;
            msg.local_address_id = addr.id();
            msg.save(tx).await?;
            msg.reload(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let d = DecryptedMessageBody::new_without_prefetching(
        String::new(),
        MessageBodyMetadata {
            local_message_id: msg.local_id,
            parsed_headers: ParsedHeaders { headers },
            ..Default::default()
        },
        MessageMimeType::TextPlain,
        None,
        "".into(),
        None,
    );

    let banners = d
        .transformed("", Default::default(), &ctx, tether)
        .await
        .body_banners;

    assert_eq!(
        vec![MessageBanner::UnsubscribeNewsletter {
            already_unsubscribed: false
        }],
        banners
    );

    d.action_unsubscribe_from_newsletter(&ctx).await.unwrap();

    let banners = d
        .transformed("", Default::default(), &ctx, tether)
        .await
        .body_banners;

    assert_eq!(
        vec![MessageBanner::UnsubscribeNewsletter {
            already_unsubscribed: true
        }],
        banners
    );

    assert_eq!(ctx.execute_all_actions().await.unwrap(), 1);
}
