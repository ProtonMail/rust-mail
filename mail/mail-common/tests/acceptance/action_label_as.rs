use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_api::services::proton::{Address as ApiAddress, Label as ApiLabel};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Label, ModelIdExtension};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    MessageCount as ApiMessageCount,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{ExclusiveLocation, SystemLabelId};
use proton_mail_common::models::{Conversation, ConversationCounters, LabelWithCounters};
use proton_mail_common::test_utils::conversations::ApiConversationTestUtils;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;
use std::collections::{HashMap, HashSet};
use velcro::{hash_map, hash_set};

#[tokio::test]
async fn action_label_as_without_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

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

    let conversation1 = ApiConversation::test_conversation("first", vec![]);
    let conversation2 =
        ApiConversation::test_conversation("second", vec![label2.clone(), label3.clone()]);
    let conversation3 =
        ApiConversation::test_conversation("third", vec![label1.clone(), label3.clone()]);
    let conversation4 = ApiConversation::test_conversation(
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
    ctx.mock_label_conversation(
        &label1_id,
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label3_id,
        vec![
            conversation2.id,
            conversation3.id.clone(),
            conversation4.id.clone(),
        ],
        vec![],
    )
    .await;
    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
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

    let conversation1 = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let conversation3 = Conversation::load(3.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation3.labels.len(), 2);
    let conversation4 = Conversation::load(4.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation4.labels.len(), 3);

    // Action
    Conversation::action_label_as(
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
    .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    // Validation
    let conversation1 = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation1.labels.len(), 1);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id()]);
    let conversation2 = Conversation::load(2.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id(), label2.id(),]);
    let conversation3 = Conversation::load(3.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation3.labels.len(), 1);
    let ids: HashSet<_> = conversation3
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id(),]);
    let conversation4 = Conversation::load(4.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation4.labels.len(), 2);
    let ids: HashSet<_> = conversation4
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id(), label2.id(),]);

    let label1 = LabelWithCounters::find_first("WHERE remote_id = ?", params!["selected"], &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label1.total_conv, 4);
    let label2 = LabelWithCounters::find_first("WHERE remote_id = ?", params!["partial"], &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label2.total_conv, 2);
    let label3 =
        LabelWithCounters::find_first("WHERE remote_id = ?", params!["unselected"], &tether)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(label3.total_conv, 0);
}

#[tokio::test]
async fn action_label_as_with_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

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

    let conversation1 = ApiConversation::test_conversation("first", vec![]);
    let conversation2 = ApiConversation::test_conversation(
        "second",
        vec![label1.clone(), label2.clone(), label3.clone()],
    );
    let conversations = vec![conversation1.clone(), conversation2.clone()];

    let params = test_init_params(labels, conversations.clone());
    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &LabelId::archive(),
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_label_conversation(&label1_id, vec![conversation1.id.clone()], None, vec![])
        .await;
    ctx.mock_unlabel_conversation(&label3_id, vec![conversation2.id], vec![])
        .await;
    ctx.catch_all().await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();

    let (label1, label2) = tether
        .tx::<_, _, StashError>(async |tx| {
            let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters1 = ConversationCounters::new(label1.id());
            counters1.total = 1;
            counters1.save(tx).await.unwrap();
            let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters2 = ConversationCounters::new(label2.id());
            counters2.total = 1;
            counters2.save(tx).await.unwrap();
            let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], tx)
                .await
                .unwrap()
                .unwrap();
            let mut counters3 = ConversationCounters::new(label3.id());
            counters3.total = 1;
            counters3.save(tx).await.unwrap();
            Ok((label1, label2))
        })
        .await
        .unwrap();

    let conversation1 = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation2.labels.len(), 3);

    // Action
    Conversation::action_label_as(
        user_ctx.action_queue(),
        inbox_label.id(),
        vec![conversation1.id(), conversation2.id()],
        vec![label1.id()],
        vec![label2.id()],
        true,
    )
    .await
    .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation
    let archive_id = Label::remote_id_counterpart(LabelId::archive(), &tether)
        .await
        .unwrap()
        .unwrap();

    let conversation1 = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation1.labels.len(), 2);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id(), archive_id]);
    assert_eq!(
        conversation1.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
    let conversation2 = Conversation::load(2.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conversation2.labels.len(), 3);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.id(), label2.id(), archive_id]);
    assert_eq!(
        conversation2.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
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
