use proton_api_mail::services::proton::response_data::IncomingDefault;
use proton_api_mail::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_core_common::models::Address;
use proton_mail_common::datatypes::MessageFlags;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::models::Conversation;
use proton_mail_common::models::default_location::IncomingDefaultLocation;
use proton_mail_test_utils::init::Params;

use proton_mail_common::models::Message;

use proton_mail_test_utils::test_context::MailTestContext;
use proton_mail_test_utils::test_context::MailUserContextTestExtension;
use stash::stash::StashError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers::{method, path};

#[tokio::test]
async fn banners() -> anyhow::Result<()> {
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

    test_ctx.mock_put_incoming_default().await;
    test_ctx.mock_post_incoming_default().await;
    test_ctx.catch_all().await;

    let ctx = test_ctx.mail_user_context().await;

    let tether = &mut ctx.user_stash().connection();

    let mut blocked_addr = Address {
        remote_id: Some("This is an id".into()),
        email: "blocked@email".into(),
        ..addr.clone()
    };

    tether
        .tx::<_, _, StashError>(async |tx| {
            conv.save(tx).await?;
            addr.save(tx).await?;
            blocked_addr.save(tx).await?;
            let incoming_default = vec![IncomingDefault {
                email: Some("blocked@email".into()),
                location: Some(ApiIncomingDefaultLocation::Blocked),
                action: None,
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
        ..Default::default()
    };
    let mut msg_phising = Message {
        flags: MessageFlags::FLAG_SUSPICIOUS,
        ..msg_normal.clone()
    };
    let mut msg_spam = Message {
        flags: MessageFlags::SPAM_AUTO,
        ..msg_normal.clone()
    };
    let mut msg_expiry = Message {
        expiration_time: 42,
        ..msg_normal.clone()
    };
    let mut msg_blocked = Message {
        local_address_id: blocked_addr.local_id.unwrap(),
        remote_address_id: blocked_addr.remote_id.unwrap(),
        local_conversation_id: conv.local_id,
        remote_conversation_id: conv.remote_id.clone(),
        ..Default::default()
    };
    tether
        .tx::<_, _, StashError>(async |tx| {
            msg_normal.save(tx).await?;
            msg_phising.save(tx).await?;
            msg_spam.save(tx).await?;
            msg_expiry.save(tx).await?;
            msg_blocked.save(tx).await?;
            Ok(())
        })
        .await?;

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_normal.get_banners(tether).await
    );
    assert_eq!(
        vec![MessageBanner::PhishingAttempt],
        msg_phising.get_banners(tether).await
    );
    assert_eq!(
        vec![MessageBanner::Spam],
        msg_spam.get_banners(tether).await
    );
    assert_eq!(
        vec![MessageBanner::Expiry { timestamp: 42 }],
        msg_expiry.get_banners(tether).await
    );
    assert_eq!(
        vec![MessageBanner::BlockedSender],
        msg_blocked.get_banners(tether).await
    );

    IncomingDefaultLocation::action_block(ctx.action_queue(), addr.local_id.unwrap()).await?;
    IncomingDefaultLocation::action_unblock(ctx.action_queue(), blocked_addr.local_id.unwrap())
        .await?;

    assert_eq!(2, ctx.execute_all_actions().await?);

    assert_eq!(
        vec![MessageBanner::BlockedSender],
        msg_normal.get_banners(tether).await
    );

    assert_eq!(
        Vec::<MessageBanner>::new(),
        msg_blocked.get_banners(tether).await
    );

    Ok(())
}
