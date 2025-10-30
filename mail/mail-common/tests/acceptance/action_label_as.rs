use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_api::services::proton::{Address as ApiAddress, Label as ApiLabel};
use proton_core_common::models::Label;
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    MessageCount as ApiMessageCount,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, ConversationCounters, LabelWithCounters};
use proton_mail_common::test_utils::conversations::ApiConversationTestUtils;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;
use stash::stash::Tether;
use std::collections::HashMap;
use velcro::hash_map;

#[tokio::test]
async fn action_label_as_without_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1.clone(), label2.clone(), label3.clone()],
    };

    let conversation1 = ApiConversation::test_conversation_in_inbox("first", vec![]);
    let conversation2 =
        ApiConversation::test_conversation_in_inbox("second", vec![label2.clone(), label3.clone()]);
    let conversation3 =
        ApiConversation::test_conversation_in_inbox("third", vec![label1.clone(), label3.clone()]);
    let conversation4 = ApiConversation::test_conversation_in_inbox(
        "fourth",
        vec![label1.clone(), label2.clone(), label3.clone()],
    );
    let conversations = vec![
        conversation1.clone(),
        conversation2.clone(),
        conversation3.clone(),
        conversation4.clone(),
    ];

    let params = test_init_params(labels, conversations.clone());
    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(&label1_id, vec![conversation1.id.clone()], None, vec![])
        .await;

    ctx.mock_label_conversation(&label1_id, vec![conversation2.id.clone()], None, vec![])
        .await;

    ctx.mock_unlabel_conversation(&label3_id, vec![conversation2.id], vec![])
        .await;
    ctx.mock_unlabel_conversation(&label3_id, vec![conversation3.id.clone()], vec![])
        .await;
    ctx.mock_unlabel_conversation(&label3_id, vec![conversation4.id.clone()], vec![])
        .await;
    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let (label1, label2) = tether
        .tx::<_, _, StashError>(async |tx| {
            let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters1 = ConversationCounters::new(label1.id());
            counters1.total = 2;
            counters1.save(tx).await.unwrap();
            let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters2 = ConversationCounters::new(label2.id());
            counters2.total = 2;
            counters2.save(tx).await.unwrap();
            let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters3 = ConversationCounters::new(label3.id());
            counters3.total = 3;
            counters3.save(tx).await.unwrap();
            Ok((label1, label2))
        })
        .await
        .unwrap();

    let [conversation1, conversation2, conversation3, conversation4] = get_convs(&tether).await;

    // The setup is:
    // - We label
    // - We undo label before executing the queue
    // - We label
    // - Back to state0
    // - We execute queue
    // - We undo label
    // - Back to state0

    assert_state0(&tether).await;

    // Action
    let undo = Conversation::action_label_as(
        &tether,
        user_ctx.action_queue(),
        inbox_label.id(),
        vec![
            conversation1.id(),
            conversation2.id(),
            conversation3.id(),
            conversation4.id(),
        ],
        vec![label1.id()],
        vec![label2.id()],
        false,
    )
    .await
    .unwrap()
    .undo;
    assert!(
        undo.is_none(),
        "undo label_as without archive must not be undoable"
    );

    assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    // user_ctx.execute_all_actions().await.unwrap();
}

#[tokio::test]
async fn action_label_as_with_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], &tether)
        .await
        .unwrap()
        .unwrap();

    let label1_id = LabelId::from("selected");
    let label1 = test_label(&label1_id, "selected");
    let label2_id = LabelId::from("partial");
    let label2 = test_label(&label2_id, "partial");
    let label3_id = LabelId::from("unselected");
    let label3 = test_label(&label3_id, "unselected");
    let labels = hash_map! {
        ApiLabelType::Label: vec![label1.clone(), label2.clone(), label3.clone()],
    };

    let conversation1 = ApiConversation::test_conversation_in_inbox("first", vec![]);
    let conversation2 =
        ApiConversation::test_conversation_in_inbox("second", vec![label2.clone(), label3.clone()]);
    let conversation3 =
        ApiConversation::test_conversation_in_inbox("third", vec![label1.clone(), label3.clone()]);
    let conversation4 = ApiConversation::test_conversation_in_inbox(
        "fourth",
        vec![label1.clone(), label2.clone(), label3.clone()],
    );
    let conversations = vec![
        conversation1.clone(),
        conversation2.clone(),
        conversation3.clone(),
        conversation4.clone(),
    ];

    let params = test_init_params(labels, conversations.clone());
    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;

    for id in [conversation1.id.clone(), conversation2.id.clone()] {
        ctx.mock_label_conversation(&label1_id, vec![id], None, vec![])
            .await;
    }

    for id in [
        conversation2.id.clone(),
        conversation3.id.clone(),
        conversation4.id.clone(),
    ] {
        ctx.mock_label_conversation(&label3_id, vec![id], None, vec![])
            .await;
    }

    for id in [
        conversation1.id.clone(),
        conversation2.id.clone(),
        conversation3.id.clone(),
        conversation4.id.clone(),
    ] {
        ctx.mock_label_conversation(&LabelId::archive(), vec![id], None, vec![])
            .await;
    }

    for id in [
        conversation1.id.clone(),
        conversation2.id.clone(),
        conversation3.id.clone(),
        conversation4.id.clone(),
    ] {
        ctx.mock_label_conversation(&LabelId::inbox(), vec![id], None, vec![])
            .await;
    }

    for id in [conversation1.id.clone(), conversation2.id.clone()] {
        ctx.mock_unlabel_conversation(&label1_id, vec![id], vec![])
            .await;
    }

    for id in [
        conversation2.id,
        conversation3.id.clone(),
        conversation4.id.clone(),
    ] {
        ctx.mock_unlabel_conversation(&label3_id, vec![id], vec![])
            .await;
    }

    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

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

    let (label1, label2) = tether
        .tx::<_, _, StashError>(async |tx| {
            let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters1 = ConversationCounters::new(label1.id());
            counters1.total = 2;
            counters1.save(tx).await.unwrap();
            let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters2 = ConversationCounters::new(label2.id());
            counters2.total = 2;
            counters2.save(tx).await.unwrap();
            let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters3 = ConversationCounters::new(label3.id());
            counters3.total = 3;
            counters3.save(tx).await.unwrap();
            Ok((label1, label2))
        })
        .await
        .unwrap();

    let [conversation1, conversation2, conversation3, conversation4] = get_convs(&tether).await;

    // The setup is:
    // - We label
    // - We undo label before executing the queue
    // - We label
    // - Back to state0
    // - We execute queue
    // - We undo label
    // - Back to state0

    assert_state0(&tether).await;
    // Action
    let undo = Conversation::action_label_as(
        &tether,
        user_ctx.action_queue(),
        inbox_label.id(),
        vec![
            conversation1.id(),
            conversation2.id(),
            conversation3.id(),
            conversation4.id(),
        ],
        vec![label1.id()],
        vec![label2.id()],
        true,
    )
    .await
    .unwrap()
    .undo
    .unwrap();

    assert_state1(&tether).await;

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    assert_state0(&tether).await;

    // Nothing ever happens because we reverted it by just cancelling the action in the queue.
    assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 0);
    assert_state0(&tether).await;

    // Action, again
    let undo = Conversation::action_label_as(
        &tether,
        user_ctx.action_queue(),
        inbox_label.id(),
        vec![
            conversation1.id(),
            conversation2.id(),
            conversation3.id(),
            conversation4.id(),
        ],
        vec![label1.id()],
        vec![label2.id()],
        true,
    )
    .await
    .unwrap()
    .undo
    .unwrap();

    assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 2);
    assert_state1(&tether).await;

    undo.undo(user_ctx.action_queue(), &mut tether)
        .await
        .unwrap();
    // Nothing ever happens because we've reverted
    assert_state0(&tether).await;
    assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 2);
    // Same local data but the api calls have been made.
    assert_state0(&tether).await;
}

fn test_init_params(
    labels: HashMap<ApiLabelType, Vec<ApiLabel>>,
    conversations: Vec<ApiConversation>,
) -> TestParams {
    let conversation_count = vec![ApiConversationCount {
        label_id: LabelId::inbox().clone(),
        total: conversations.len() as u64,
        unread: 0,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone(),
        total: 1,
        unread: 0,
    }];
    TestParams {
        labels,
        addresses: vec![ApiAddress::test_address()],
        conversations,
        conversation_count,
        message_count,
        ..Default::default()
    }
}

fn test_label(label_id: &LabelId, name: &str) -> ApiLabel {
    ApiLabel {
        id: label_id.clone(),
        label_type: ApiLabelType::Label,
        name: name.to_owned(),
        ..ApiLabel::test_default()
    }
}

async fn get_convs(tether: &Tether) -> [Conversation; 4] {
    Conversation::find("WHERE local_id IN (1,2,3,4)", vec![], tether)
        .await
        .unwrap()
        .try_into()
        .unwrap()
}

fn label_eq<'a>(conv: &Conversation, comp: impl IntoIterator<Item = &'a LabelWithCounters>) {
    let labels = conv
        .labels
        .iter()
        .map(|x| x.local_label_id.unwrap())
        .sorted();

    let mut other_labels = comp.into_iter().collect::<Vec<_>>();
    other_labels.sort_by(|l1, l2| l1.local_id.unwrap().cmp(&l2.local_id.unwrap()));
    for (conv, label) in labels.zip(other_labels.into_iter()) {
        assert_eq!(conv, label.label().id());
    }
}

/// State 0: No action has been made
async fn assert_state0(tether: &Tether) {
    let [conversation1, conversation2, conversation3, conversation4] = get_convs(tether).await;
    assert_eq!(conversation1.labels.len(), 1);
    assert_eq!(conversation2.labels.len(), 3);
    assert_eq!(conversation3.labels.len(), 3);
    assert_eq!(conversation4.labels.len(), 4);
}

/// State 1: Action has been made in test2
async fn assert_state1(tether: &Tether) {
    let label1 = LabelWithCounters::from_remote_ids(tether, [LabelId::from("selected")])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let label2 = LabelWithCounters::from_remote_ids(tether, [LabelId::from("partial")])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let label3 = LabelWithCounters::from_remote_ids(tether, [LabelId::from("unselected")])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let archive = LabelWithCounters::from_remote_ids(tether, [LabelId::archive()])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let [conversation1, conversation2, conversation3, conversation4] = get_convs(tether).await;

    assert_eq!(label1.total_conv, 4);
    assert_eq!(label2.total_conv, 2);
    assert_eq!(label3.total_conv, 0);
    assert_eq!(archive.total_conv, 4);

    label_eq(&conversation1, [&archive, &label1]);
    label_eq(&conversation2, [&archive, &label1, &label2]);
    label_eq(&conversation3, [&archive, &label1]);
    label_eq(&conversation4, [&archive, &label1, &label2]);
}
