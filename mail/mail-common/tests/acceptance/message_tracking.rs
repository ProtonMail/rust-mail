use proton_core_api::services::proton::{AddressId, LabelId, UserId};
use proton_core_common::datatypes::UnixTimestamp;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    ViewMode as ApiViewMode,
};
use proton_mail_common::datatypes::{LocalMessageId, SystemLabelId};
use proton_mail_common::models::{MessageTracker, MessageTrackerUrl};
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{Mailbox, TrackerDetector};
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;
use tokio::time::timeout;

const TEST_USER_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";

#[derive(Clone)]
struct TrackerTableWatcher {
    sender: flume::Sender<()>,
}

impl TrackerTableWatcher {
    fn new() -> (Self, flume::Receiver<()>) {
        let (sender, receiver) = flume::unbounded();
        (Self { sender }, receiver)
    }
}

impl TableObserver for TrackerTableWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            MessageTracker::table_name().to_string(),
            MessageTrackerUrl::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for TrackerTableWatcher: {:?}",
                    e
                );
            })
            .ok();
    }
}

async fn wait_for_tracker_tables(
    receiver: &flume::Receiver<()>,
    timeout_duration: Duration,
) -> anyhow::Result<()> {
    match timeout(timeout_duration, receiver.recv_async()).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(_)) => anyhow::bail!("Channel closed"),
        Err(_) => anyhow::bail!("Timeout waiting for table changes"),
    }
}

fn test_params() -> Params {
    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params
}

fn test_message() -> ApiMessage {
    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from("test_message_id"),
            conversation_id: ConversationId::from("test_conversation_id"),
            order: 0,
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            label_ids: vec![LabelId::inbox()],
            external_id: None,
            subject: "Test Message".to_owned(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            flags: ApiMessageFlags::empty(),
            time: 1715863508,
            size: 100,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
        body: ApiMessageBody {
            header: String::new(),
            parsed_headers: Default::default(),
            body: String::new(),
            mime_type: Default::default(),
            attachments: vec![],
            reply_to: Default::default(),
            reply_tos: vec![],
        },
    }
}

#[tokio::test]
async fn check_message_trackers_with_empty_urls() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let (watcher, receiver) = TrackerTableWatcher::new();
    let _handle = tether
        .subscribe_to(move |_| Box::new(watcher.clone()))
        .unwrap();

    let tracker_detector = user_ctx.get_service::<TrackerDetector>();
    let message_id: LocalMessageId = 1.into();
    let urls = HashSet::new();

    tracker_detector
        .check_message_trackers(message_id, urls)
        .await
        .unwrap();

    wait_for_tracker_tables(&receiver, Duration::from_secs(5))
        .await
        .expect("Timeout waiting for tracker tables");

    let _tracked = MessageTracker::load(message_id, &tether)
        .await
        .unwrap()
        .expect("TrackedMessage should exist");

    let tracking_urls = MessageTrackerUrl::find_by_message(message_id, &tether)
        .await
        .unwrap();
    assert!(tracking_urls.is_empty());
}

#[tokio::test]
async fn check_message_trackers_with_non_tracker_urls() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    ctx.mock_proxy_img_dry_run("https://safe-image.example.com/logo.png")
        .await;
    ctx.mock_proxy_img_dry_run("https://example.com/image.jpg")
        .await;

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let (watcher, receiver) = TrackerTableWatcher::new();
    let _handle = tether
        .subscribe_to(move |_| Box::new(watcher.clone()))
        .unwrap();

    let tracker_detector = user_ctx.get_service::<TrackerDetector>();
    let message_id: LocalMessageId = 1.into();
    let mut urls = HashSet::new();
    urls.insert("https://safe-image.example.com/logo.png".to_string());
    urls.insert("https://example.com/image.jpg".to_string());

    tracker_detector
        .check_message_trackers(message_id, urls)
        .await
        .unwrap();

    wait_for_tracker_tables(&receiver, Duration::from_secs(5))
        .await
        .expect("Timeout waiting for tracker tables");

    let _tracked = MessageTracker::load(message_id, &tether)
        .await
        .unwrap()
        .expect("TrackedMessage should exist");

    let tracking_urls = MessageTrackerUrl::find_by_message(message_id, &tether)
        .await
        .unwrap();
    assert!(tracking_urls.is_empty());
}

#[tokio::test]
async fn check_message_trackers_with_single_tracker() {
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

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let (watcher, receiver) = TrackerTableWatcher::new();
    let _handle = tether
        .subscribe_to(move |_| Box::new(watcher.clone()))
        .unwrap();

    let tracker_detector = user_ctx.get_service::<TrackerDetector>();
    let message_id: LocalMessageId = 1.into();
    let mut urls = HashSet::new();
    urls.insert("https://tracker.example.com/pixel.gif".to_string());

    tracker_detector
        .check_message_trackers(message_id, urls)
        .await
        .unwrap();

    wait_for_tracker_tables(&receiver, Duration::from_secs(5))
        .await
        .expect("Timeout waiting for tracker tables");

    let _tracked = MessageTracker::load(message_id, &tether)
        .await
        .unwrap()
        .expect("TrackedMessage should exist");

    let tracking_urls = MessageTrackerUrl::find_by_message(message_id, &tether)
        .await
        .unwrap();

    let expected = vec![MessageTrackerUrl {
        id: tracking_urls.first().and_then(|u| u.id),
        local_message_id: message_id,
        tracker_domain: "tracker.example.com".to_string(),
        original_url: "https://tracker.example.com/pixel.gif".to_string(),
    }];
    assert_eq!(tracking_urls, expected);
}

#[tokio::test]
async fn check_message_trackers_with_mixed_urls() {
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

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let (watcher, receiver) = TrackerTableWatcher::new();
    let _handle = tether
        .subscribe_to(move |_| Box::new(watcher.clone()))
        .unwrap();

    let tracker_detector = user_ctx.get_service::<TrackerDetector>();
    let message_id: LocalMessageId = 1.into();
    let mut urls = HashSet::new();
    urls.insert("https://tracker.example.com/pixel.gif".to_string());
    urls.insert("https://safe-image.example.com/logo.png".to_string());

    tracker_detector
        .check_message_trackers(message_id, urls)
        .await
        .unwrap();

    wait_for_tracker_tables(&receiver, Duration::from_secs(5))
        .await
        .expect("Timeout waiting for tracker tables");

    let _tracked = MessageTracker::load(message_id, &tether)
        .await
        .unwrap()
        .expect("TrackedMessage should exist");

    let tracking_urls = MessageTrackerUrl::find_by_message(message_id, &tether)
        .await
        .unwrap();

    let expected = vec![MessageTrackerUrl {
        id: tracking_urls.first().and_then(|u| u.id),
        local_message_id: message_id,
        tracker_domain: "tracker.example.com".to_string(),
        original_url: "https://tracker.example.com/pixel.gif".to_string(),
    }];
    assert_eq!(tracking_urls, expected);
}

#[tokio::test]
async fn check_message_trackers_with_multiple_trackers() {
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

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let (watcher, receiver) = TrackerTableWatcher::new();
    let _handle = tether
        .subscribe_to(move |_| Box::new(watcher.clone()))
        .unwrap();

    let tracker_detector = user_ctx.get_service::<TrackerDetector>();
    let message_id: LocalMessageId = 1.into();
    let mut urls = HashSet::new();
    urls.insert("https://tracker1.example.com/pixel.gif".to_string());
    urls.insert("https://tracker2.example.com/beacon.png".to_string());
    urls.insert("https://tracker1.example.com/another.gif".to_string());

    tracker_detector
        .check_message_trackers(message_id, urls)
        .await
        .unwrap();

    wait_for_tracker_tables(&receiver, Duration::from_secs(5))
        .await
        .expect("Timeout waiting for tracker tables");

    let _tracked = MessageTracker::load(message_id, &tether)
        .await
        .unwrap()
        .expect("TrackedMessage should exist");

    let tracking_urls = MessageTrackerUrl::find_by_message(message_id, &tether)
        .await
        .unwrap();

    assert_eq!(tracking_urls.len(), 3);

    let urls: BTreeSet<_> = tracking_urls
        .iter()
        .map(|t| (t.tracker_domain.as_str(), t.original_url.as_str()))
        .collect();

    let expected_urls = BTreeSet::from([
        (
            "tracker1.example.com",
            "https://tracker1.example.com/pixel.gif",
        ),
        (
            "tracker2.example.com",
            "https://tracker2.example.com/beacon.png",
        ),
        (
            "tracker1.example.com",
            "https://tracker1.example.com/another.gif",
        ),
    ]);

    assert_eq!(urls, expected_urls);
}

#[tokio::test]
async fn get_tracker_info_returns_correct_data() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let message = test_message();
    ctx.setup_user(test_params()).await;
    ctx.mock_get_messages()
        .respond_with(vec![message.metadata.clone()])
        .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
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

    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let message_id: LocalMessageId = 1.into();

    let tracker_info = TrackerDetector::get_tracker_info(message_id, &tether)
        .await
        .unwrap();
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

    let tracker_info = TrackerDetector::get_tracker_info(message_id, &tether)
        .await
        .unwrap();
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

    let tracker_info = TrackerDetector::get_tracker_info(message_id, &tether)
        .await
        .unwrap()
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
