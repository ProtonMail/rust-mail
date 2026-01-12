use crate::acceptance::message_privacy::common::get_or_wait_for_privacy_data;

use super::common::{create_message_with_html_body, test_params};
use proton_core_api::services::proton::{LabelId, UserId};
use proton_mail_common::datatypes::{SystemLabelId, UTMLink};
use proton_mail_common::decrypted_message::TransformOpts;
use proton_mail_common::models::{Message, MessageUtmLinkUrl};
use proton_mail_common::test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{Mailbox, TrackerService};
use stash::orm::Model;
use velcro::btree_set;

#[tokio::test]
async fn utm_links_are_cleaned_when_decrypting_message() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <p>Check out our <a href="https://example.com/?utm_source=newsletter&utm_medium=email&product=shoes">latest products</a>!</p>
        <p>Visit <a href="https://store.com/?utm_campaign=summer_sale&item=123&utm_content=banner">our store</a>.</p>
    </body></html>"#;

    let message = create_message_with_html_body("utm_msg_1", html_body);

    ctx.setup_user(test_params()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let decrypted_body = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let transformed = decrypted_body
        .transformed(
            "test@example.com",
            TransformOpts::default(),
            &user_ctx,
            &tether,
        )
        .await;

    eprintln!("Transformed body: {}", transformed.body);

    assert!(
        transformed
            .body
            .contains("https://example.com/?product=shoes"),
        "First link should have UTM parameters stripped but keep product param. Got: {}",
        transformed.body
    );
    assert!(
        transformed.body.contains("https://store.com/?item=123"),
        "Second link should have UTM parameters stripped but keep item param"
    );
    assert!(
        !transformed.body.contains("utm_source"),
        "utm_source should be removed"
    );
    assert!(
        !transformed.body.contains("utm_medium"),
        "utm_medium should be removed"
    );
    assert!(
        !transformed.body.contains("utm_campaign"),
        "utm_campaign should be removed"
    );
    assert!(
        !transformed.body.contains("utm_content"),
        "utm_content should be removed"
    );
}

#[tokio::test]
async fn cleaned_utm_links_summary_can_be_retrieved() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <a href="https://shop.com/?utm_source=email&utm_medium=newsletter&category=electronics">Electronics</a>
        <a href="https://blog.com/?utm_campaign=spring2024&post=123">Read more</a>
    </body></html>"#;

    let message = create_message_with_html_body("utm_msg_2", html_body);

    ctx.setup_user(test_params()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let service = user_ctx.get_service::<TrackerService>();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let decrypted_body = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed(
            "test@example.com",
            TransformOpts::default(),
            &user_ctx,
            &tether,
        )
        .await;

    let (_, utm_info) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();
    assert_eq!(utm_info.links.len(), 2, "Should have 2 cleaned links");

    let links_vec: Vec<_> = utm_info.links.iter().collect();

    assert!(
        links_vec[0].original_url.contains("utm_") && links_vec[1].original_url.contains("utm_"),
        "Original URLs should contain UTM parameters"
    );
    assert!(
        !links_vec[0].cleaned_url.contains("utm_") && !links_vec[1].cleaned_url.contains("utm_"),
        "Cleaned URLs should not contain UTM parameters"
    );
}

#[tokio::test]
async fn utm_watcher_is_notified_on_message_decryption() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <a href="https://example.org/?utm_source=test&ref=homepage">Click here</a>
    </body></html>"#;

    let message = create_message_with_html_body("utm_msg_3", html_body);

    ctx.setup_user(test_params()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let service = user_ctx.get_service::<TrackerService>();
    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let decrypted_body = saved_message
        .fetch_message_body(
            &user_ctx,
            &mut user_ctx.user_stash().connection().await.unwrap(),
        )
        .await
        .unwrap();

    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed(
            "test@example.com",
            TransformOpts::default(),
            &user_ctx,
            &tether,
        )
        .await;

    let (_, utm_info) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    assert_eq!(
        utm_info.links,
        btree_set! {
           UTMLink {
               original_url: "https://example.org/?utm_source=test&ref=homepage".to_string(),
               cleaned_url: "https://example.org/?ref=homepage".to_string(),
           }
        }
    );
}

#[tokio::test]
async fn utm_results_are_cached_in_database() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <a href="https://cached.example/?utm_campaign=cache_test&page=home">Home</a>
    </body></html>"#;

    let message = create_message_with_html_body("utm_msg_4", html_body);

    ctx.setup_user(test_params()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let service = user_ctx.get_service::<TrackerService>();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let decrypted_body = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let watch = service.watch(1.into()).await.unwrap();

    let _transformed = decrypted_body
        .transformed(
            "test@example.com",
            TransformOpts::default(),
            &user_ctx,
            &tether,
        )
        .await;

    let (_, first_fetch) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    let second_fetch = service.get_info(1.into()).await.unwrap();

    assert!(
        second_fetch.utm_links.is_some(),
        "Second fetch should have UTM info"
    );

    let second_links = second_fetch.utm_links.unwrap();

    assert_eq!(
        first_fetch.links.len(),
        second_links.links.len(),
        "Both fetches should return same number of links"
    );
    assert_eq!(
        first_fetch.links, second_links.links,
        "Cached results should be identical"
    );

    let utm_urls = MessageUtmLinkUrl::find_by_message(1.into(), &tether)
        .await
        .unwrap();
    assert_eq!(
        utm_urls.len(),
        1,
        "Should have exactly one cached entry (not duplicated)"
    );
}
