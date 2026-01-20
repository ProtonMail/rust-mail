use crate::acceptance::message_privacy::common::get_or_wait_for_privacy_data;

use super::common::{create_message_with_html_body, test_params};
use proton_core_api::services::proton::{LabelId, UserId};
use proton_core_common::datatypes::UnixTimestamp;
use proton_mail_common::datatypes::{LocalMessageId, SystemLabelId, TrackerDomain};
use proton_mail_common::decrypted_message::TransformOpts;
use proton_mail_common::models::{Message, MessageTracker, MessageTrackerUrl};
use proton_mail_common::test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{Mailbox, TrackerService};
use stash::orm::Model;
use std::collections::BTreeSet;
use velcro::btree_set;

#[tokio::test]
async fn image_trackers_detected_with_no_images() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <p>This message has no images at all.</p>
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_1", html_body);

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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };

    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;
    let (tracker_info, _utm_info) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    assert!(tracker_info.trackers.is_empty());
}

#[tokio::test]
async fn image_trackers_detected_with_safe_images() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.mock_proxy_img_dry_run("https://safe-image.example.com/logo.png")
        .await;
    ctx.mock_proxy_img_dry_run("https://example.com/image.jpg")
        .await;

    let html_body = r#"<html><body>
        <img src="https://safe-image.example.com/logo.png" alt="Logo" />
        <img src="https://example.com/image.jpg" alt="Photo" />
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_2", html_body);

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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };
    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;

    let (tracker_info, _utm_info) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    assert!(tracker_info.trackers.is_empty());
}

#[tokio::test]
async fn image_trackers_detected_with_single_tracker() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.mock_proxy_img_dry_run_tracked(
        "https://tracker.example.com/pixel.gif",
        "tracker.example.com",
    )
    .await;

    let html_body = r#"<html><body>
        <img src="https://tracker.example.com/pixel.gif" alt="" />
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_3", html_body);

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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };
    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;

    let (tracker_info, _) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();
    assert_eq!(
        tracker_info.trackers,
        btree_set! {
            TrackerDomain {
                name: "tracker.example.com".into(),
                urls: btree_set!{
                    "https://tracker.example.com/pixel.gif".into()
                }
            }
        }
    );
}

#[tokio::test]
async fn image_trackers_detected_with_mixed_images() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.mock_proxy_img_dry_run_tracked(
        "https://tracker.example.com/pixel.gif",
        "tracker.example.com",
    )
    .await;
    ctx.mock_proxy_img_dry_run("https://safe-image.example.com/logo.png")
        .await;

    let html_body = r#"<html><body>
        <img src="https://tracker.example.com/pixel.gif" alt="" />
        <img src="https://safe-image.example.com/logo.png" alt="Logo" />
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_4", html_body);

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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };
    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;

    let (tracker_info, _) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    assert_eq!(
        tracker_info.trackers,
        btree_set! {
            TrackerDomain {
                name: "tracker.example.com".into(),
                urls: btree_set!{
                    "https://tracker.example.com/pixel.gif".into()
                }
            }
        }
    );
}

#[tokio::test]
async fn image_trackers_detected_with_multiple_trackers() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.mock_proxy_img_dry_run_tracked(
        "https://tracker1.example.com/pixel.gif",
        "tracker1.example.com",
    )
    .await;
    ctx.mock_proxy_img_dry_run_tracked(
        "https://tracker2.example.com/beacon.png",
        "tracker2.example.com",
    )
    .await;
    ctx.mock_proxy_img_dry_run_tracked(
        "https://tracker1.example.com/another.gif",
        "tracker1.example.com",
    )
    .await;

    let html_body = r#"<html><body>
        <img src="https://tracker1.example.com/pixel.gif" alt="" />
        <img src="https://tracker2.example.com/beacon.png" alt="" />
        <img src="https://tracker1.example.com/another.gif" alt="" />
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_5", html_body);

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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };
    let watch = service.watch(1.into()).await.unwrap();
    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;

    let (tracker_info, _) = get_or_wait_for_privacy_data(1.into(), service, watch)
        .await
        .unwrap();

    assert_eq!(
        tracker_info.trackers,
        btree_set! {
            TrackerDomain {
                name: "tracker1.example.com".into(),
                urls: btree_set!{
                    "https://tracker1.example.com/pixel.gif".into(),
                    "https://tracker1.example.com/another.gif".into(),
                }
            },
            TrackerDomain {
                name: "tracker2.example.com".into(),
                urls: btree_set!{
                    "https://tracker2.example.com/beacon.png".into()
                }
            }
        }
    );
}

#[tokio::test]
async fn get_tracker_info_returns_correct_data() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let message = create_message_with_html_body("tracker_msg_info", "<html><body></body></html>");

    ctx.setup_user(test_params()).await;
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

    let message_id: LocalMessageId = 1.into();

    let tracker_info = user_ctx
        .get_service::<TrackerService>()
        .get_info(message_id)
        .await
        .unwrap()
        .trackers;
    assert!(tracker_info.is_none());

    tether
        .tx(async |tx| {
            MessageTracker {
                local_message_id: message_id,
                last_checked_at: UnixTimestamp::now(),
            }
            .save(tx)
            .await
        })
        .await
        .unwrap();

    let tracker_info = user_ctx
        .get_service::<TrackerService>()
        .get_info(message_id)
        .await
        .unwrap()
        .trackers;
    assert!(tracker_info.unwrap().trackers.is_empty());

    tether
        .tx(async |tx| {
            MessageTracker {
                local_message_id: message_id,
                last_checked_at: UnixTimestamp::now(),
            }
            .save(tx)
            .await?;

            MessageTrackerUrl {
                id: None,
                local_message_id: message_id,
                tracker_domain: "tracker1.com".to_string(),
                original_url: "https://tracker1.com/pixel1.gif".to_string(),
            }
            .save(tx)
            .await?;

            MessageTrackerUrl {
                id: None,
                local_message_id: message_id,
                tracker_domain: "tracker1.com".to_string(),
                original_url: "https://tracker1.com/pixel2.gif".to_string(),
            }
            .save(tx)
            .await?;

            MessageTrackerUrl {
                id: None,
                local_message_id: message_id,
                tracker_domain: "tracker2.com".to_string(),
                original_url: "https://tracker2.com/beacon.png".to_string(),
            }
            .save(tx)
            .await
        })
        .await
        .unwrap();

    let tracker_info = user_ctx
        .get_service::<TrackerService>()
        .get_info(message_id)
        .await
        .unwrap()
        .trackers
        .unwrap();

    assert_eq!(tracker_info.trackers.len(), 2);

    let mut tracker_iter = tracker_info.trackers.iter();
    let tracker1 = tracker_iter.next().unwrap();
    assert_eq!(tracker1.name, "tracker1.com");
    assert_eq!(tracker1.urls.len(), 2);
    let expected_urls1 = BTreeSet::from([
        "https://tracker1.com/pixel1.gif".to_string(),
        "https://tracker1.com/pixel2.gif".to_string(),
    ]);
    assert_eq!(tracker1.urls, expected_urls1);

    let tracker2 = tracker_iter.next().unwrap();
    assert_eq!(tracker2.name, "tracker2.com");
    assert_eq!(tracker2.urls.len(), 1);
    let expected_urls2 = BTreeSet::from(["https://tracker2.com/beacon.png".to_string()]);
    assert_eq!(tracker2.urls, expected_urls2);
}

#[tokio::test]
async fn image_trackers_not_checked_when_proxy_disabled() {
    use super::common::test_params_proxy_disabled;

    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let html_body = r#"<html><body>
        <img src="https://tracker.example.com/pixel.gif" alt="" />
    </body></html>"#;

    let message = create_message_with_html_body("tracker_msg_proxy_disabled", html_body);

    ctx.setup_user(test_params_proxy_disabled()).await;
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

    let transform_opts = TransformOpts {
        hide_remote_images: Some(true),
        ..Default::default()
    };

    let _transformed = decrypted_body
        .transformed("test@example.com", transform_opts, &user_ctx, &tether)
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let privacy_info = service.get_info(1.into()).await.unwrap();

    assert!(privacy_info.trackers.is_none());
}
