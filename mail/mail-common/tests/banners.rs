use proton_core_api::services::proton::LabelId;
use proton_core_common::models::Address;
use proton_core_common::models::Label;
use proton_core_common::models::ModelIdExtension as _;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::IncomingDefault;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_mail_common::datatypes::MessageFlags;
use proton_mail_common::datatypes::SystemLabelId as _;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::models::Conversation;
use proton_mail_common::models::MessageBodyMetadata;
use proton_mail_common::models::default_location::IncomingDefaultLocation;
use proton_mail_test_utils::init::Params;

use proton_mail_common::models::Message;

use proton_mail_test_utils::test_context::MailTestContext;
use proton_mail_test_utils::test_context::MailUserContextTestExtension;
use stash::orm::Model;
use stash::stash::StashError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers::{method, path};

#[tokio::test]
async fn banners() {
    let test_ctx = MailTestContext::new().await;
    #[allow(
        clippy::redundant_closure_call,
        reason = "IIFE so that we can use `?` without wiremock being annoying"
    )]
    (async || {
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

        test_ctx.mock_report_phishing().await;
        test_ctx.mock_delete_incoming_default().await;
        test_ctx
            .mock_post_incoming_default(IncomingDefault {
                email: Some("normal@email".to_string()),
                ..Default::default()
            })
            .await;
        test_ctx.mock_put_message_ham(&"spam".into()).await;
        test_ctx.mock_put_message_ham(&"phishing".into()).await;
        test_ctx.mock_put_message_ham(&"normal".into()).await;
        test_ctx.catch_all().await;

        let ctx = test_ctx.mail_user_context().await;
        test_ctx.initialize_uninitialized_ctx(&ctx).await;

        let tether = &mut ctx.user_stash().connection();

        tether
            .tx::<_, _, StashError>(async |tx| {
                conv.save(tx).await?;
                addr.save(tx).await?;
                let incoming_default = vec![IncomingDefault {
                    email: Some("blocked@email".into()),
                    location: Some(ApiIncomingDefaultLocation::Blocked),
                    id: "123".into(),
                    action: None,
                    domain: None,
                }];
                IncomingDefaultLocation::store_by_email(incoming_default, tx).await?;
                Ok(())
            })
            .await?;

        let mut msg_normal = Message {
            local_conversation_id: conv.local_id,
            remote_conversation_id: conv.remote_id.clone(),
            local_address_id: addr.local_id.unwrap(),
            remote_address_id: addr.remote_id.clone().unwrap(),
            remote_id: Some(message_id),
            sender: proton_mail_common::datatypes::MessageSender {
                address: "normal@email".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut msg_phishing = Message {
            flags: MessageFlags::PHISHING_AUTO,
            remote_id: Some("phishing".into()),
            ..msg_normal.clone()
        };
        let mut msg_spam = Message {
            flags: MessageFlags::SPAM_AUTO,
            remote_id: Some("spam".into()),
            ..msg_normal.clone()
        };

        let mut msg_sus = Message {
            flags: MessageFlags::FLAG_SUSPICIOUS,
            remote_id: Some("sus".into()),
            ..msg_normal.clone()
        };

        let msg_expiry = Message {
            expiration_time: 42,
            remote_id: Some("expiry".into()),
            ..msg_normal.clone()
        };
        let msg_blocked = Message {
            remote_id: Some("blocked".into()),
            sender: proton_mail_common::datatypes::MessageSender {
                address: "blocked@email".to_string(),
                ..Default::default()
            },
            ..msg_normal.clone()
        };

        let scheduled_time = 123456_u64;
        let msg_schedule_send = Message {
            label_ids: vec![LabelId::all_scheduled()],
            time: scheduled_time,
            ..Default::default()
        };

        assert_eq!(
            Vec::<MessageBanner>::new(),
            msg_normal.get_banners(tether).await
        );
        assert_eq!(
            vec![MessageBanner::Expiry { timestamp: 42 }],
            msg_expiry.get_banners(tether).await
        );
        assert_eq!(
            vec![MessageBanner::BlockedSender],
            msg_blocked.get_banners(tether).await
        );
        assert_eq!(
            vec![MessageBanner::ScheduledSend {
                timestamp: scheduled_time
            }],
            msg_schedule_send.get_banners(tether).await
        );

        let (msg_normal_id, msg_spam_id, msg_phishing_id, msg_sus_id) = tether
            .tx::<_, _, anyhow::Error>(async |tx| {
                msg_normal.save(tx).await?;
                msg_spam.save(tx).await?;
                msg_phishing.save(tx).await?;
                msg_sus.save(tx).await?;

                MessageBodyMetadata {
                    local_message_id: msg_normal.local_id,
                    remote_message_id: msg_normal.remote_id.clone(),
                    ..Default::default()
                }
                .save(tx)
                .await?;
                Message::store_decrypted_message_body(
                    msg_normal.local_id.unwrap(),
                    "im a nigerian prince, click this link".into(),
                    tx,
                )
                .await?;

                let spam = Label::remote_id_counterpart(LabelId::spam(), tx)
                    .await?
                    .unwrap();
                let inbox = Label::remote_id_counterpart(LabelId::inbox(), tx)
                    .await?
                    .unwrap();

                Message::move_messages(
                    inbox,
                    spam,
                    vec![
                        msg_phishing.local_id.unwrap(),
                        msg_spam.local_id.unwrap(),
                        msg_sus.local_id.unwrap(),
                    ],
                    tx,
                )
                .await?;

                Ok((
                    msg_normal.local_id.unwrap(),
                    msg_spam.local_id.unwrap(),
                    msg_phishing.local_id.unwrap(),
                    msg_sus.local_id.unwrap(),
                ))
            })
            .await?;

        let msg_normal = Message::load(msg_normal_id, tether).await?.unwrap();
        let msg_spam = Message::load(msg_spam_id, tether).await?.unwrap();
        let msg_phishing = Message::load(msg_phishing_id, tether).await?.unwrap();
        let msg_sus = Message::load(msg_sus_id, tether).await?.unwrap();

        assert_eq!(
            Vec::<MessageBanner>::new(),
            msg_normal.get_banners(tether).await
        );
        assert_eq!(
            vec![MessageBanner::PhishingAttempt],
            msg_phishing.get_banners(tether).await
        );
        assert_eq!(
            vec![MessageBanner::Spam],
            msg_spam.get_banners(tether).await
        );
        assert_eq!(
            Vec::<MessageBanner>::new(),
            msg_sus.get_banners(tether).await,
            "sus messages don't warrant a banner"
        );

        // Let's block, unblock and report phishing to see how labels change

        IncomingDefaultLocation::action_unblock(ctx.action_queue(), "blocked@email".to_string())
            .await?;
        Message::action_ham(ctx.action_queue(), vec![msg_spam_id, msg_phishing_id]).await?;

        let inbox = Label::remote_id_counterpart(LabelId::inbox(), tether)
            .await?
            .unwrap();

        Message::action_report_phishing(ctx.action_queue(), inbox, msg_normal_id).await?;

        assert_eq!(3, ctx.execute_all_actions().await?);

        let msg_normal = Message::load(msg_normal_id, tether).await?.unwrap();
        let msg_spam = Message::load(msg_spam_id, tether).await?.unwrap();
        let msg_phishing = Message::load(msg_phishing_id, tether).await?.unwrap();

        assert_eq!(
            Vec::<MessageBanner>::new(),
            msg_spam.get_banners(tether).await
        );

        assert_eq!(
            vec![MessageBanner::PhishingAttempt],
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
        IncomingDefaultLocation::action_block(ctx.action_queue(), "normal@email".to_string())
            .await?;

        Message::action_ham(ctx.action_queue(), vec![msg_normal.local_id.unwrap()]).await?;

        let msg_normal = Message::load(msg_normal_id, tether).await?.unwrap();
        assert_eq!(
            vec![MessageBanner::BlockedSender],
            msg_normal.get_banners(tether).await
        );

        assert_eq!(2, ctx.execute_all_actions().await?);

        Ok::<_, anyhow::Error>(())
    })()
    .await
    .unwrap();
}
