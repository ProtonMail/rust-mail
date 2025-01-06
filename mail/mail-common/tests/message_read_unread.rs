use itertools::Itertools;
use proton_api_core::services::proton::common::{LabelId, RemoteId as ApiRemoteId};
use proton_api_mail::services::proton::response_data::MailSettings as ApiMailSettings;
use proton_api_mail::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_api_mail::services::proton::response_data::ViewMode as ApiViewMode;
use proton_core_common::datatypes::{IdCounterpart, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Label, Message};
use proton_mail_common::Mailbox;
use proton_mail_test_utils::init::Params;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::params;
use std::sync::LazyLock;
use test_case::test_case;

struct TestItem {
    id: &'static str,
    to_mark: bool,
    unread: bool,
}

static EMPTY: LazyLock<Vec<TestItem>> = LazyLock::new(Vec::new);
static ALL_UNREAD: LazyLock<Vec<TestItem>> = LazyLock::new(|| {
    vec![
        TestItem {
            id: "one",
            to_mark: true,
            unread: true,
        },
        TestItem {
            id: "two",
            to_mark: true,
            unread: true,
        },
        TestItem {
            id: "three",
            to_mark: false,
            unread: false,
        },
        TestItem {
            id: "four",
            to_mark: false,
            unread: true,
        },
    ]
});
static MIXED_UNREAD: LazyLock<Vec<TestItem>> = LazyLock::new(|| {
    vec![
        TestItem {
            id: "one",
            to_mark: true,
            unread: true,
        },
        TestItem {
            id: "two",
            to_mark: true,
            unread: false,
        },
        TestItem {
            id: "three",
            to_mark: false,
            unread: false,
        },
        TestItem {
            id: "four",
            to_mark: false,
            unread: true,
        },
    ]
});
static ALL_READ: LazyLock<Vec<TestItem>> = LazyLock::new(|| {
    vec![
        TestItem {
            id: "one",
            to_mark: true,
            unread: false,
        },
        TestItem {
            id: "two",
            to_mark: true,
            unread: false,
        },
        TestItem {
            id: "three",
            to_mark: false,
            unread: false,
        },
        TestItem {
            id: "four",
            to_mark: false,
            unread: true,
        },
    ]
});

#[test_case(&EMPTY, 0; "empty")]
#[test_case(&ALL_UNREAD, 1; "all unread")]
#[test_case(&MIXED_UNREAD, 1; "mixed unread")]
#[test_case(&ALL_READ, 1; "all read")]
#[tokio::test]
async fn mark_message_read(messages: &[TestItem], expected_unread: usize) {
    // Setup
    // * Create all given messages in stash
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = messages.len() as u64;

    let to_mark = messages
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = messages
        .iter()
        .filter(|m| m.unread && m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    let messages = messages.iter().map(test_message(&params)).collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages(messages.clone()).await;
    if !expected_to_mark.is_empty() {
        ctx.mock_put_messages_read(expected_to_mark, vec![]).await;
    }
    ctx.catch_all().await;

    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    if !messages.is_empty() {
        let mut conversation =
            Conversation::find_by_id(params.conversations[0].id.clone(), &tether)
                .await
                .unwrap()
                .unwrap();
        conversation.num_unread = messages.len() as u64;
        let bond = tether.transaction().await.unwrap();
        conversation.save(&bond).await.unwrap();
        bond.commit().await.unwrap();
    }

    // Action
    let message_ids = RemoteId::counterparts::<Message>(to_mark, &tether)
        .await
        .unwrap();
    user_ctx
        .with_queue(|queue| Message::action_mark_read(queue, inbox.local_id.unwrap(), message_ids))
        .await
        .unwrap();

    // Validation
    let messages = Message::find("WHERE unread = ?", params![true], &tether)
        .await
        .unwrap();
    assert_eq!(messages.len(), expected_unread);
}

#[test_case(&EMPTY, 0; "empty")]
#[test_case(&ALL_UNREAD, 3; "all unread")]
#[test_case(&MIXED_UNREAD, 3; "mixed unread")]
#[test_case(&ALL_READ, 3; "all read")]
#[tokio::test]
async fn mark_message_unread(messages: &[TestItem], expected_unread: usize) {
    // Setup
    // * Create all given messages in stash
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = messages.len() as u64;

    let to_mark = messages
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = messages
        .iter()
        .filter(|m| !m.unread && m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let messages = messages.iter().map(test_message(&params)).collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages(messages.clone()).await;
    if !expected_to_mark.is_empty() {
        ctx.mock_put_messages_unread(expected_to_mark, vec![]).await;
    }
    ctx.catch_all().await;

    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    // Action
    let message_ids = RemoteId::counterparts::<Message>(to_mark, &tether)
        .await
        .unwrap();
    user_ctx
        .with_queue(|queue| {
            Message::action_mark_unread(queue, inbox.local_id.unwrap(), message_ids)
        })
        .await
        .unwrap();

    // Validation
    let messages = Message::find("WHERE unread = ?", params![true], &tether)
        .await
        .unwrap();
    assert_eq!(messages.len(), expected_unread);
}

fn test_message(params: &Params) -> impl FnMut(&TestItem) -> ApiMessageMetadata {
    let conversation_id = params.conversations[0].id.clone();
    let address_id = params.addresses[0].id.clone();
    move |message| {
        let TestItem {
            id: name, unread, ..
        } = message;
        ApiMessageMetadata {
            id: ApiRemoteId::from(name.to_owned()),
            conversation_id: conversation_id.clone(),
            address_id: address_id.clone(),
            unread: *unread,
            ..Default::default()
        }
    }
}
