use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_common::cache::CacheMessageKey;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Message;
use proton_mail_common::Mailbox;
<<<<<<< HEAD
use proton_mail_test_utils::common::TestContext;
use proton_mail_test_utils::message_body::{
    message_body_test_message_simple, message_body_test_params, message_body_test_user_secret,
    TEST_MESSAGE_BODY_DECRYPTED, TEST_USER_ID,
};
=======
>>>>>>> 3e8d6e58 (Fixed rust formatter issues.)
use proton_test_utils::mail::message_body::*;
use proton_test_utils::test_context::TestContext;
use stash::orm::Model;
use std::io::read_to_string;

#[tokio::test]
async fn mailbox_message_body_simple() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = message_body_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let message = message_body_test_message_simple();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    // Resolve local id.
    let saved_message = Message::load(1.into(), user_ctx.user_stash())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(saved_message.remote_id, Some(message.metadata.id.into()));

    // No message cached
    let cache = user_ctx.messages_cache();
    assert!(cache.is_empty());

    // Decrypt the message body.
    let pgp_provider = new_pgp_provider();
    let _local_id = saved_message.local_id.unwrap();
    let address_id = saved_message.remote_address_id.clone();
    let address_keys = user_ctx
        .unlocked_address_keys(&pgp_provider, &address_id)
        .await
        .unwrap();
    let api = user_ctx.session().api();
    let decrypted_body = saved_message
        .fetch_message_body(
            cache,
            address_keys.clone(),
            pgp_provider,
            api,
            user_ctx.user_stash(),
        )
        .await
        .unwrap();

    assert_eq!(decrypted_body.body, TEST_MESSAGE_BODY_DECRYPTED);

    // Now a message is cached and it's the right one
    assert_eq!(cache.len(), 1);
    let key = CacheMessageKey::from_message(&saved_message, user_ctx.user_stash());
    let item = cache
        .get_item(&key)
        .unwrap()
        .map(|f| read_to_string(f).unwrap());
    assert_eq!(item, Some(TEST_MESSAGE_BODY_DECRYPTED.to_owned()));

    let pgp_provider = new_pgp_provider();
    // Only one call to API is done
    saved_message
        .fetch_message_body(
            cache,
            address_keys,
            pgp_provider,
            api,
            user_ctx.user_stash(),
        )
        .await
        .unwrap();
    assert_eq!(cache.len(), 1);
}
