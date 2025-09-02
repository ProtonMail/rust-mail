use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::response_data::Conversation as ApiConversation;
use proton_mail_api::services::proton::response_data::ConversationLabel as ApiConversationLabel;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Conversation;
use proton_mail_common::models::ConversationCounters;
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
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
#[test_case(&ALL_UNREAD, 3; "all unread")]
#[test_case(&MIXED_UNREAD, 3; "mixed unread")]
#[test_case(&ALL_READ, 3; "all read")]
#[tokio::test]
async fn mark_conversation_read(conversations: &[TestItem], expected_read: usize) {
    // Setup
    // * Create all conversations in stash
    let ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.conversation_count[0].total = conversations.len() as u64;
    params.conversations = conversations.iter().map(test_conversation).collect_vec();

    let to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = conversations
        .iter()
        .filter(|m| m.unread && m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(params.conversations, 1).await;
    for id in expected_to_mark {
        ctx.mock_mark_conversation_read(vec![id], vec![]).await;
    }
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    // Action
    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();
    let mut counters = ConversationCounters::find_by_id(inbox.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    counters.unread = counters.total;
    tether.tx(async |tx| counters.save(tx).await).await.unwrap();

    let conversation_ids = Conversation::remote_ids_counterpart(to_mark, &tether)
        .await
        .unwrap();
    Conversation::action_mark_read(user_ctx.action_queue(), inbox.id(), conversation_ids)
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation
    let conversations = Conversation::find("WHERE num_unread = ?", params![0], &tether)
        .await
        .unwrap();
    assert_eq!(conversations.len(), expected_read);
}

#[test_case(&EMPTY, 0; "empty")]
#[test_case(&ALL_UNREAD, 1; "all unread")]
#[test_case(&MIXED_UNREAD, 1; "mixed unread")]
#[test_case(&ALL_READ, 1; "all read")]
#[tokio::test]
async fn mark_conversation_unread(conversations: &[TestItem], expected_read: usize) {
    // Setup
    // * Create all conversations in stash
    let ctx = MailTestContext::new().await;

    let mut params = Params::default_basic();
    params.conversation_count[0].total = conversations.len() as u64;
    params.conversations = conversations.iter().map(test_conversation).collect_vec();

    let to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = conversations
        .iter()
        .filter(|m| !m.unread && m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(params.conversations, 1).await;
    for id in expected_to_mark {
        ctx.mock_mark_conversation_unread(vec![id], LabelId::inbox(), vec![])
            .await;
    }
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    // Action
    let inbox = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();
    let mut counters = ConversationCounters::find_by_id(inbox.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    counters.unread = counters.total;
    tether.tx(async |tx| counters.save(tx).await).await.unwrap();

    let conversation_ids = Conversation::remote_ids_counterpart(to_mark, &tether)
        .await
        .unwrap();
    Conversation::action_mark_unread(user_ctx.action_queue(), inbox.id(), conversation_ids)
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation
    let conversations = Conversation::find("WHERE num_unread = ?", params![0], &tether)
        .await
        .unwrap();
    assert_eq!(conversations.len(), expected_read);
}

fn test_conversation(conversation: &TestItem) -> ApiConversation {
    let TestItem {
        id: name, unread, ..
    } = conversation;
    ApiConversation {
        id: ConversationId::from(name.to_owned()),
        num_messages: 1,
        num_unread: if *unread { 1 } else { 0 },
        labels: vec![ApiConversationLabel {
            id: LabelId::inbox(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 1,
            context_num_unread: if *unread { 1 } else { 0 },
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        ..ApiConversation::test_default()
    }
}
