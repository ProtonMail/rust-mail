use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_api::services::proton::{Address as ApiAddress, Label as ApiLabel};
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    MessageCount as ApiMessageCount,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, ConversationCounters, LabelWithCounters, Message};
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

    let conv_message_id = |id: &ConversationId| MessageId::from(format!("msg-{id:?}"));

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

            // Create messages (required for undo)
            let addr_id = ApiAddress::test_address().id;
            let local_addr_id = Address::remote_id_counterpart(addr_id.clone(), tx)
                .await
                .unwrap()
                .unwrap();
            for conv in get_convs(tx).await {
                Message {
                    remote_conversation_id: conv.remote_id.clone(),
                    local_conversation_id: conv.local_id,
                    local_address_id: local_addr_id,
                    remote_address_id: addr_id.clone(),
                    remote_id: Some(conv_message_id(conv.remote_id.as_ref().unwrap())),
                    label_ids: conv
                        .labels
                        .iter()
                        .map(|l| l.remote_label_id.clone().unwrap())
                        .collect(),
                    ..Message::test_default()
                }
                .save(tx)
                .await
                .unwrap();
            }
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
        undo.is_some(),
        "undo label_as without archive must be undoable"
    );

    assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
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

    let conv_message_id = |id: &ConversationId| MessageId::from(format!("msg-{id:?}"));

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

    ctx.mock_unlabel_messages(
        &LabelId::archive(),
        [
            &conversation1,
            &conversation2,
            &conversation3,
            &conversation4,
        ]
        .iter()
        .map(|c| conv_message_id(&c.id))
        .collect(),
        vec![],
    )
    .await;

    ctx.mock_label_messages(
        &LabelId::inbox(),
        [
            &conversation1,
            &conversation2,
            &conversation3,
            &conversation4,
        ]
        .iter()
        .map(|c| conv_message_id(&c.id))
        .collect(),
    )
    .await;

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

            // Create messages (required for conversation move undo)
            let addr_id = ApiAddress::test_address().id;
            let local_addr_id = Address::remote_id_counterpart(addr_id.clone(), tx)
                .await
                .unwrap()
                .unwrap();
            for conv in get_convs(tx).await {
                Message {
                    remote_conversation_id: conv.remote_id.clone(),
                    local_conversation_id: conv.local_id,
                    local_address_id: local_addr_id,
                    remote_address_id: addr_id.clone(),
                    remote_id: Some(conv_message_id(conv.remote_id.as_ref().unwrap())),
                    label_ids: conv
                        .labels
                        .iter()
                        .map(|l| l.remote_label_id.clone().unwrap())
                        .collect(),
                    ..Message::test_default()
                }
                .save(tx)
                .await
                .unwrap();
            }

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

mod rebase {
    use super::*;
    use pretty_assertions::{assert_eq, assert_ne};
    use proton_action_queue::action::ActionGroup;
    use proton_action_queue::rebase::RebaseChangeSet;
    use proton_core_common::datatypes::LocalLabelId;
    use proton_core_common::models::ModelExtension;
    use proton_mail_common::datatypes::ConversationViewOptions;
    use proton_mail_common::models::ConversationLabel;
    use proton_mail_common::test_utils::scroller::StoreLabeledModelMap;
    use proton_mail_common::{MailUserContext, conv_id, conversation, message};
    use std::sync::Arc;

    // NOTE: The must_archive rebase is handled by the message/conv move rules.
    fn custom_label_id1() -> LabelId {
        LabelId::from("Custom1")
    }

    fn custom_label_id2() -> LabelId {
        LabelId::from("Custom2")
    }

    fn custom_label_id3() -> LabelId {
        LabelId::from("Custom3")
    }

    fn conv_msg_id(conv: usize, msg: usize) -> MessageId {
        MessageId::from(format!("conv{conv}-msg{msg}"))
    }
    async fn local_label_id(label_id: LabelId, tether: &Tether) -> LocalLabelId {
        Label::remote_id_counterpart(label_id, tether)
            .await
            .unwrap()
            .unwrap()
    }

    async fn setup() -> (
        MailTestContext,
        Arc<MailUserContext>,
        Conversation,
        Conversation,
    ) {
        setup_with_mocks(async |_, _, _| {}).await
    }

    async fn setup_with_mocks(
        mk_mocks: impl AsyncFnOnce(&MailTestContext, &Conversation, &Conversation),
    ) -> (
        MailTestContext,
        Arc<MailUserContext>,
        Conversation,
        Conversation,
    ) {
        let ctx = MailTestContext::new().await;
        let mut params = TestParams::default_basic();
        params.labels.entry(ApiLabelType::Folder).or_insert(vec![
            ApiLabel {
                id: custom_label_id1(),
                parent_id: None,
                color: "".to_string(),
                display: false,
                expanded: false,
                label_type: ApiLabelType::Label,
                name: "Custom1".to_string(),
                notify: false,
                order: 0,
                path: None,
                sticky: false,
            },
            ApiLabel {
                id: custom_label_id2(),
                parent_id: None,
                color: "".to_string(),
                display: false,
                expanded: false,
                label_type: ApiLabelType::Label,
                name: "Custom2".to_string(),
                notify: false,
                order: 0,
                path: None,
                sticky: false,
            },
            ApiLabel {
                id: custom_label_id3(),
                parent_id: None,
                color: "".to_string(),
                display: false,
                expanded: false,
                label_type: ApiLabelType::Label,
                name: "Custom3".to_string(),
                notify: false,
                order: 0,
                path: None,
                sticky: false,
            },
        ]);
        ctx.setup_user(params.clone()).await;
        let user_ctx = ctx.mail_user_context().await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let mut conv_data1 = hash_map! {
            vec![LabelId::inbox(),custom_label_id1(),custom_label_id2()]: vec![
                conversation!(remote_id: conv_id!("my_conv"),
            labels: vec![ConversationLabel{remote_label_id:Some(LabelId::all_mail()), ..ConversationLabel::test_default()}]),
            ]
        };
        conv_data1.save_to_database(tether).await;

        let mut conv_data2 = hash_map! {
            vec![LabelId::inbox(), custom_label_id2()]: vec![
                conversation!(remote_id: conv_id!("my_conv2"),
            labels: vec![ConversationLabel{remote_label_id:Some(LabelId::all_mail()), ..ConversationLabel::test_default()}]),
            ]
        };
        conv_data2.save_to_database(tether).await;
        let conv1 = &conv_data1
            .get(&vec![
                LabelId::inbox(),
                custom_label_id1(),
                custom_label_id2(),
            ])
            .unwrap()[0];

        // Message with unread, custom label.
        let mut msg_data = hash_map! {
            vec![LabelId::inbox(), custom_label_id1()]:
            vec![message!(
                    remote_id: Some(conv_msg_id(1,1)),
                    local_conversation_id: conv1.local_id,
                    remote_conversation_id: conv1.remote_id.clone(),
                    label_ids:vec![LabelId::all_mail(), LabelId::almost_all_mail()],
                    time:100.into(),
                    unread:true
            )],
            vec![LabelId::inbox(), custom_label_id2()]:
            vec![message!(
                    remote_id: Some(conv_msg_id(1,2)),
                    local_conversation_id: conv1.local_id,
                    remote_conversation_id: conv1.remote_id.clone(),
                    label_ids:vec![LabelId::all_mail(), LabelId::almost_all_mail()],
                    time:200.into(),
                    unread:true
            )],
        };
        msg_data.save_to_database(tether).await;

        let conv2 = &conv_data2
            .get(&vec![LabelId::inbox(), custom_label_id2()])
            .unwrap()[0];
        let mut msg_data = hash_map! {
        vec![LabelId::inbox(), custom_label_id2()]:
        vec![message!(
                remote_id: Some(conv_msg_id(2,1)),
                local_conversation_id: conv2.local_id,
                remote_conversation_id: conv2.remote_id.clone(),
                label_ids:vec![LabelId::all_mail(), LabelId::almost_all_mail()],
                unread:false
        )]};
        msg_data.save_to_database(tether).await;

        let conv1 = Conversation::find_by_id(conv1.id(), tether)
            .await
            .unwrap()
            .unwrap();
        let conv2 = Conversation::find_by_id(conv2.id(), tether)
            .await
            .unwrap()
            .unwrap();
        mk_mocks(&ctx, &conv1, &conv2).await;

        (ctx, user_ctx, conv1, conv2)
    }

    #[tokio::test]
    async fn simple() {
        let (_test_ctx, user_ctx, mut original_conv, _) = setup().await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let local_inbox = local_label_id(LabelId::inbox(), tether).await;
        let local_custom_label_id3 = local_label_id(custom_label_id3(), tether).await;

        let mut original_messages =
            Message::in_conversation(original_conv.id(), ConversationViewOptions::All, tether)
                .await
                .unwrap();

        let undo = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv.id()],
            vec![local_custom_label_id3],
            vec![],
            false,
        )
        .await
        .unwrap();

        let labeled_conv = Conversation::find_by_id(original_conv.id(), tether)
            .await
            .unwrap()
            .unwrap();

        let labeled_msg1 = Message::find_by_remote_id(conv_msg_id(1, 1), tether)
            .await
            .unwrap()
            .unwrap();
        let labeled_msg2 = Message::find_by_remote_id(conv_msg_id(1, 2), tether)
            .await
            .unwrap()
            .unwrap();

        assert!(labeled_msg1.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_msg1.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_msg1.label_ids.contains(&custom_label_id2()));
        assert!(labeled_msg2.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_msg2.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_msg2.label_ids.contains(&custom_label_id2()));

        // simulate state reset.
        tether
            .tx(async |tx| {
                for msg in &mut original_messages {
                    msg.save(tx).await?;
                }
                original_conv.save(tx).await
            })
            .await
            .unwrap();

        let rebase_change_set = RebaseChangeSet::from(original_conv.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &rebase_change_set)
            .await
            .unwrap();

        let rebased_conv = Conversation::find_by_id(original_conv.id(), tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(rebased_conv, labeled_conv);
        assert_ne!(rebased_conv, original_conv);

        undo.undo
            .unwrap()
            .undo(user_ctx.action_queue(), tether)
            .await
            .unwrap();

        let undoed_msg1 = Message::find_by_remote_id(conv_msg_id(1, 1), tether)
            .await
            .unwrap()
            .unwrap();
        let undoed_msg2 = Message::find_by_remote_id(conv_msg_id(1, 2), tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!undoed_msg1.label_ids.contains(&custom_label_id3()));
        assert!(undoed_msg1.label_ids.contains(&custom_label_id1()));
        assert!(!undoed_msg1.label_ids.contains(&custom_label_id2()));
        assert!(!undoed_msg2.label_ids.contains(&custom_label_id3()));
        assert!(!undoed_msg2.label_ids.contains(&custom_label_id1()));
        assert!(undoed_msg2.label_ids.contains(&custom_label_id2()));

        let mut undoed_conv = Conversation::find_by_id(original_conv.id(), tether)
            .await
            .unwrap()
            .unwrap();

        original_conv.sort_labels();
        undoed_conv.sort_labels();

        assert_eq!(original_conv, undoed_conv);
    }

    #[tokio::test]
    async fn rebase_to_same_state_still_runs_server_requests() {
        let (_test_ctx, user_ctx, original_conv, _) = setup_with_mocks(async |ctx, conv1, _| {
            ctx.mock_label_conversation(
                &custom_label_id3(),
                vec![conv1.remote_id.clone().unwrap()],
                None,
                vec![],
            )
            .await;

            ctx.mock_unlabel_conversation(
                &custom_label_id1(),
                vec![conv1.remote_id.clone().unwrap()],
                vec![],
            )
            .await;

            ctx.mock_unlabel_conversation(
                &custom_label_id2(),
                vec![conv1.remote_id.clone().unwrap()],
                vec![],
            )
            .await;
        })
        .await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let local_inbox = local_label_id(LabelId::inbox(), tether).await;
        let local_custom_label_id3 = local_label_id(custom_label_id3(), tether).await;

        let _ = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv.id()],
            vec![local_custom_label_id3],
            vec![],
            false,
        )
        .await
        .unwrap();

        let rebase_change_set = RebaseChangeSet::from(original_conv.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &rebase_change_set)
            .await
            .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn rebase_reverts_to_last_updated_state() {
        let (_test_ctx, user_ctx, original_conv, _) = setup().await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let local_inbox = local_label_id(LabelId::inbox(), tether).await;
        let local_custom_label_id1 = local_label_id(custom_label_id1(), tether).await;
        let local_custom_label_id2 = local_label_id(custom_label_id2(), tether).await;
        let local_custom_label_id3 = local_label_id(custom_label_id3(), tether).await;

        let original_messages =
            Message::in_conversation(original_conv.id(), ConversationViewOptions::All, tether)
                .await
                .unwrap();

        let undo = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv.id()],
            vec![local_custom_label_id3],
            vec![],
            false,
        )
        .await
        .unwrap();

        let labeled_msg1 = Message::find_by_remote_id(conv_msg_id(1, 1), tether)
            .await
            .unwrap()
            .unwrap();
        let labeled_msg2 = Message::find_by_remote_id(conv_msg_id(1, 2), tether)
            .await
            .unwrap()
            .unwrap();

        assert!(labeled_msg1.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_msg1.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_msg1.label_ids.contains(&custom_label_id2()));
        assert!(labeled_msg2.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_msg2.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_msg2.label_ids.contains(&custom_label_id2()));

        let mut updated_conv = original_conv.clone();
        updated_conv.labels.retain(|l| {
            ![local_custom_label_id1, local_custom_label_id2].contains(&l.local_label_id.unwrap())
        });

        let mut updated_messages = original_messages
            .iter()
            .cloned()
            .map(|mut m| {
                m.label_ids
                    .retain(|l| ![custom_label_id2(), custom_label_id1()].contains(l));
                m
            })
            .collect::<Vec<_>>();

        // simulate state reset.
        tether
            .tx(async |tx| {
                for msg in &mut updated_messages {
                    msg.save(tx).await?;
                }
                updated_conv.save(tx).await
            })
            .await
            .unwrap();

        updated_conv.reload(tether).await.unwrap();
        let rebase_change_set = RebaseChangeSet::from(original_conv.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &rebase_change_set)
            .await
            .unwrap();

        undo.undo
            .unwrap()
            .undo(user_ctx.action_queue(), tether)
            .await
            .unwrap();

        let undoed_msg1 = Message::find_by_remote_id(conv_msg_id(1, 1), tether)
            .await
            .unwrap()
            .unwrap();
        let undoed_msg2 = Message::find_by_remote_id(conv_msg_id(1, 2), tether)
            .await
            .unwrap()
            .unwrap();

        assert!(!undoed_msg1.label_ids.contains(&custom_label_id3()));
        assert!(!undoed_msg1.label_ids.contains(&custom_label_id1()));
        assert!(!undoed_msg1.label_ids.contains(&custom_label_id2()));
        assert!(!undoed_msg2.label_ids.contains(&custom_label_id3()));
        assert!(!undoed_msg2.label_ids.contains(&custom_label_id1()));
        assert!(!undoed_msg2.label_ids.contains(&custom_label_id2()));

        let undoed_conv = Conversation::find_by_id(original_conv.id(), tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_conv, undoed_conv);
    }

    #[tokio::test]
    async fn rebase_only_targets_modified_items() {
        let (_test_ctx, user_ctx, mut original_conv1, mut original_conv2) = setup().await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let local_inbox = local_label_id(LabelId::inbox(), tether).await;
        let local_custom_label_id3 = local_label_id(custom_label_id3(), tether).await;

        let mut original_conv1_messages =
            Message::in_conversation(original_conv1.id(), ConversationViewOptions::All, tether)
                .await
                .unwrap();

        let mut original_conv2_messages =
            Message::in_conversation(original_conv2.id(), ConversationViewOptions::All, tether)
                .await
                .unwrap();

        let _ = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv1.id(), original_conv2.id()],
            vec![local_custom_label_id3],
            vec![],
            false,
        )
        .await
        .unwrap();

        let labeled_conv1 = Conversation::find_by_id(original_conv1.id(), tether)
            .await
            .unwrap()
            .unwrap();

        let labeled_conv2 = Conversation::find_by_id(original_conv1.id(), tether)
            .await
            .unwrap()
            .unwrap();

        let labeled_conv1_msg1 = Message::find_by_remote_id(conv_msg_id(1, 1), tether)
            .await
            .unwrap()
            .unwrap();
        let labeled_conv1_msg2 = Message::find_by_remote_id(conv_msg_id(1, 2), tether)
            .await
            .unwrap()
            .unwrap();

        assert!(labeled_conv1_msg1.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_conv1_msg1.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_conv1_msg1.label_ids.contains(&custom_label_id2()));
        assert!(labeled_conv1_msg2.label_ids.contains(&custom_label_id3()));
        assert!(!labeled_conv1_msg2.label_ids.contains(&custom_label_id1()));
        assert!(!labeled_conv1_msg2.label_ids.contains(&custom_label_id2()));

        // simulate state reset.
        tether
            .tx(async |tx| {
                for msg in &mut original_conv1_messages {
                    msg.save(tx).await?;
                }
                for msg in &mut original_conv2_messages {
                    msg.save(tx).await?;
                }
                original_conv1.save(tx).await?;
                original_conv2.save(tx).await
            })
            .await
            .unwrap();

        let rebase_change_set = RebaseChangeSet::from(original_conv1.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &rebase_change_set)
            .await
            .unwrap();

        let rebased_conv1 = Conversation::find_by_id(original_conv1.id(), tether)
            .await
            .unwrap()
            .unwrap();

        let rebased_conv2 = Conversation::find_by_id(original_conv2.id(), tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rebased_conv1, labeled_conv1);
        assert_ne!(rebased_conv1, original_conv1);
        assert_ne!(rebased_conv2, labeled_conv2);
        assert_eq!(rebased_conv2, original_conv2);
    }

    #[tokio::test]
    async fn rebase_stack_still_applies_all_state_if_current_is_up_to_date() {
        let (_test_ctx, user_ctx, original_conv, _) = setup_with_mocks(async |ctx, conv1, _| {
            ctx.mock_label_conversation(
                &custom_label_id3(),
                vec![conv1.remote_id.clone().unwrap()],
                None,
                vec![],
            )
            .await;

            ctx.mock_unlabel_conversation(
                &custom_label_id1(),
                vec![conv1.remote_id.clone().unwrap()],
                vec![],
            )
            .await;

            ctx.mock_unlabel_conversation(
                &custom_label_id2(),
                vec![conv1.remote_id.clone().unwrap()],
                vec![],
            )
            .await;

            ctx.mock_unlabel_conversation(
                &custom_label_id3(),
                vec![conv1.remote_id.clone().unwrap()],
                vec![],
            )
            .await;
        })
        .await;

        let tether = &mut user_ctx.user_stash().connection().await.unwrap();

        let local_inbox = local_label_id(LabelId::inbox(), tether).await;
        let local_custom_label_id3 = local_label_id(custom_label_id3(), tether).await;

        let _ = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv.id()],
            vec![local_custom_label_id3],
            vec![],
            false,
        )
        .await
        .unwrap();

        let _ = Conversation::action_label_as(
            tether,
            user_ctx.action_queue(),
            local_inbox,
            vec![original_conv.id()],
            vec![],
            vec![],
            false,
        )
        .await
        .unwrap();

        let rebase_change_set = RebaseChangeSet::from(original_conv.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &rebase_change_set)
            .await
            .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 2);
    }
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
        assert_eq!(conv, label.id());
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
