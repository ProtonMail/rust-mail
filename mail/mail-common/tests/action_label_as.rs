use proton_api_core::services::proton::response_data::Address as ApiAddress;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount, Label as ApiLabel,
    MessageCount as ApiMessageCount,
};
use proton_core_common::datatypes::{Id, LabelId};
use proton_core_test_utils::addresses::ApiAddressTestUtils;
use proton_mail_common::datatypes::{ExclusiveLocation, SystemLabel, SystemLabelId};
use proton_mail_common::models::{Conversation, Label};
use proton_mail_common::Mailbox;
use proton_mail_test_utils::conversations::ApiConversationTestUtils;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use stash::params;
use std::collections::{HashMap, HashSet};
use velcro::{hash_map, hash_set};

#[tokio::test]
async fn action_label_as_without_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
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
        &label1_id.clone().into_inner().into(),
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label3_id.into_inner().into(),
        vec![
            conversation2.id,
            conversation3.id.clone(),
            conversation4.id.clone(),
        ],
        vec![],
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    let tx = stash.transaction().await.unwrap();
    let mut label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], &tx)
        .await
        .unwrap()
        .unwrap();
    label1.total_conv = 2;
    label1.save(&tx).await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], &tx)
        .await
        .unwrap()
        .unwrap();
    label2.total_conv = 2;
    label2.save(&tx).await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], &tx)
        .await
        .unwrap()
        .unwrap();
    label3.total_conv = 3;
    label3.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let conversation3 = Conversation::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation3.labels.len(), 2);
    let conversation4 = Conversation::load(4.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation4.labels.len(), 3);

    // Action
    Conversation::action_label_as(
        user_ctx.queue(),
        inbox_label.local_id.unwrap(),
        vec![
            conversation1.local_id.unwrap(),
            conversation2.local_id.unwrap(),
            conversation3.local_id.unwrap(),
            conversation4.local_id.unwrap(),
        ],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        false,
    )
    .await
    .unwrap();

    // Validation
    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation1.labels.len(), 1);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap()]);
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 2);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![label1.local_id.unwrap(), label2.local_id.unwrap(),]
    );
    let conversation3 = Conversation::load(3.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation3.labels.len(), 1);
    let ids: HashSet<_> = conversation3
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap(),]);
    let conversation4 = Conversation::load(4.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation4.labels.len(), 2);
    let ids: HashSet<_> = conversation4
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![label1.local_id.unwrap(), label2.local_id.unwrap(),]
    );

    let label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label1.total_conv, 4);
    let label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label2.total_conv, 2);
    let label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(label3.total_conv, 0);
}

#[tokio::test]
async fn action_label_as_with_archive() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();

    let inbox_label = Label::find_first("WHERE remote_id = ?", params![LabelId::inbox()], stash)
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
        &LabelId::archive().into(),
        vec![conversation1.id.clone(), conversation2.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_label_conversation(
        &label1_id.clone().into_inner().into(),
        vec![conversation1.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label3_id.into_inner().into(),
        vec![conversation2.id],
        vec![],
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();
    mailbox.sync(10).await.unwrap();

    let tx = stash.transaction().await.unwrap();
    let mut label1 = Label::find_first("WHERE remote_id = ?", params!["selected"], &tx)
        .await
        .unwrap()
        .unwrap();
    label1.total_conv = 1;
    label1.save(&tx).await.unwrap();
    let mut label2 = Label::find_first("WHERE remote_id = ?", params!["partial"], &tx)
        .await
        .unwrap()
        .unwrap();
    label2.total_conv = 1;
    label2.save(&tx).await.unwrap();
    let mut label3 = Label::find_first("WHERE remote_id = ?", params!["unselected"], &tx)
        .await
        .unwrap()
        .unwrap();
    label3.total_conv = 1;
    label3.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert!(conversation1.labels.is_empty());
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 3);

    // Action
    Conversation::action_label_as(
        user_ctx.queue(),
        inbox_label.local_id.unwrap(),
        vec![
            conversation1.local_id.unwrap(),
            conversation2.local_id.unwrap(),
        ],
        vec![label1.local_id.unwrap()],
        vec![label2.local_id.unwrap()],
        true,
    )
    .await
    .unwrap();

    // Validation
    let archive_id = LabelId::archive()
        .counterpart::<Label, _>(stash)
        .await
        .unwrap()
        .unwrap();

    let conversation1 = Conversation::load(1.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation1.labels.len(), 2);
    let ids: HashSet<_> = conversation1
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(ids, hash_set![label1.local_id.unwrap(), archive_id]);
    assert_eq!(
        conversation1.exclusive_location,
        Some(ExclusiveLocation::System {
            name: SystemLabel::Archive,
            local_id: archive_id,
        })
    );
    let conversation2 = Conversation::load(2.into(), stash).await.unwrap().unwrap();
    assert_eq!(conversation2.labels.len(), 3);
    let ids: HashSet<_> = conversation2
        .labels
        .iter()
        .map(|l| l.local_label_id.unwrap())
        .collect();
    assert_eq!(
        ids,
        hash_set![
            label1.local_id.unwrap(),
            label2.local_id.unwrap(),
            archive_id
        ]
    );
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
        label_id: LabelId::inbox().clone().into(),
        total: conversations.len() as u64,
        unread: 0,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone().into(),
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
        id: label_id.clone().into(),
        label_type: ApiLabelType::Label,
        name: name.to_owned(),
        ..Default::default()
    }
}
