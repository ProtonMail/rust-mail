use itertools::Itertools;
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::prelude::MessageId;
use proton_mail_api::services::proton::response_data::Conversation as ApiConversation;
use proton_mail_api::services::proton::response_data::ConversationLabel as ApiConversationLabel;
use proton_mail_api::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::ConversationCounters;
use proton_mail_common::models::{Conversation, Message};
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
    let messages = conversations.iter().map(test_message).collect_vec();

    let to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(params.conversations, 1).await;
    for id in expected_to_mark {
        ctx.mock_mark_conversation_read(vec![id], vec![]).await;
    }

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
    tether
        .tx(async |tx| {
            Message::create_or_update_messages_from_metadata(messages, None, tx)
                .await
                .unwrap();
            counters.save(tx).await
        })
        .await
        .unwrap();

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
    let messages = conversations.iter().map(test_message).collect_vec();

    let to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let expected_to_mark = conversations
        .iter()
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(params.conversations, 1).await;
    for id in expected_to_mark {
        ctx.mock_mark_conversation_unread(vec![id], LabelId::inbox(), vec![])
            .await;
    }

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
    tether
        .tx(async |tx| {
            Message::create_or_update_messages_from_metadata(messages, None, tx)
                .await
                .unwrap();
            counters.save(tx).await
        })
        .await
        .unwrap();

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

fn test_message(conversation: &TestItem) -> ApiMessageMetadata {
    let TestItem {
        id: name, unread, ..
    } = conversation;

    let addr_id = AddressId::from(TEST_ADDRESS_ID);

    ApiMessageMetadata {
        id: MessageId::from(format!("conv-{}-msg", *name)),
        conversation_id: ConversationId::from(name.to_owned()),
        address_id: addr_id,
        unread: *unread,
        label_ids: vec![LabelId::inbox()],
        ..ApiMessageMetadata::test_default()
    }
}

mod rebase {
    use super::*;
    use pretty_assertions::{assert_eq, assert_ne};
    use proton_action_queue::action::ActionGroup;
    use proton_action_queue::rebase::RebaseChangeSet;
    use proton_core_api::services::proton::{Action, AddressId, EventId};
    use proton_core_common::models::{Address, ModelExtension};
    use proton_core_common::services::global_feature_flags::MAIL_ET_REBASE_FEATURE_KEY;
    use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
    use proton_mail_api::services::proton::common::ConversationId;
    use proton_mail_api::services::proton::prelude::{ConversationEvent, MailEvent, MessageEvent};
    use proton_mail_api::services::proton::response_data::MessageFlags;
    use proton_mail_common::MailUserContext;
    use proton_mail_common::datatypes::ConversationViewOptions;
    use proton_mail_common::models::ConversationLabel;
    use stash::stash::StashError;
    use std::sync::Arc;

    async fn setup_with_mocks(
        unread: Vec<Vec<bool>>,
        mk_mocks: impl AsyncFnOnce(&MailTestContext, &[Conversation]),
    ) -> (MailTestContext, Arc<MailUserContext>, Vec<Conversation>) {
        let ctx = MailTestContext::new().await;
        let user_ctx = ctx.uninitialized_mail_user_context().await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();
        let params = Params::default_basic();
        ctx.setup_user(params.clone()).await;
        ctx.initialize_uninitialized_ctx(&user_ctx).await;

        let addr_id = AddressId::from(TEST_ADDRESS_ID);
        let local_addr_id = Address::remote_id_counterpart(addr_id.clone(), &tether)
            .await
            .unwrap()
            .unwrap();

        let mut messages = Vec::new();

        let mut conversations =
            unread
                .into_iter()
                .enumerate()
                .map(|(idx, unread)| {
                    let conv_id = ConversationId::from(format!("conv-{idx}"));
                    let unread_count = unread.iter().filter(|v| **v).count() as u64;

                    let conversation = Conversation {
                        remote_id: Some(conv_id.clone()),
                        labels: vec![ConversationLabel {
                            remote_label_id: Some(LabelId::inbox()),
                            context_num_messages: unread.len() as u64,
                            context_num_unread: unread_count,
                            ..ConversationLabel::test_default()
                        }],
                        has_messages: true,
                        is_known: true,
                        num_unread: unread_count,
                        num_messages: unread.len() as u64,
                        ..Conversation::test_default()
                    };

                    messages.extend(unread.into_iter().enumerate().map(|(msg_idx, unread)| {
                        Message {
                            remote_id: Some(MessageId::from(format!("conv-{idx}-msg-{msg_idx}"))),
                            remote_conversation_id: Some(conv_id.clone()),
                            remote_address_id: addr_id.clone(),
                            local_address_id: local_addr_id,
                            label_ids: vec![LabelId::inbox()],
                            unread,
                            time: ((msg_idx * 100) as u64).into(),
                            ..Message::test_default()
                        }
                    }));

                    conversation
                })
                .collect_vec();

        tether
            .tx::<_, _, StashError>(async |tx| {
                for conv in &mut conversations {
                    conv.save(tx).await?;
                }
                for message in &mut messages {
                    message.save(tx).await?;
                    message.reload(tx).await?;
                }
                Ok(())
            })
            .await
            .unwrap();

        mk_mocks(&ctx, &conversations).await;

        (ctx, user_ctx, conversations)
    }

    #[tokio::test]
    async fn simple_mark_read() {
        let (_test_ctx, user_ctx, conversations) =
            setup_with_mocks(vec![vec![true, false, true]], async |ctx, conv| {
                ctx.mock_mark_conversation_read(vec![conv[0].remote_id.clone().unwrap()], vec![])
                    .await
            })
            .await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();
        let local_inbox_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap();

        let mut original_conversation = conversations.into_iter().next().unwrap();
        let mut original_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        let queued = Conversation::action_mark_read(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();
        let updated_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let updated_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(updated_messages.iter().filter(|m| m.unread).count(), 0);
        assert_eq!(updated_conversation.num_unread, 0);
        assert_eq!(updated_conversation.labels[0].context_num_unread, 0);

        // reset state
        tether
            .tx(async |tx| {
                for msg in &mut original_messages {
                    msg.save(tx).await.unwrap();
                }
                original_conversation.save(tx).await
            })
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_conversation.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let rebased_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(rebased_conversation, updated_conversation);
        assert_ne!(rebased_conversation, original_conversation);
        assert_eq!(rebased_messages, updated_messages);

        user_ctx.action_queue().cancel(queued.id).await.unwrap();
        let reverted_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let reverted_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(reverted_conversation, original_conversation);
        assert_eq!(reverted_messages, original_messages);

        Conversation::action_mark_read(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn create_event_tracked_in_rebase_changeset() {
        let (_test_ctx, user_ctx, conversations) =
            setup_with_mocks(vec![vec![true]], async |_, _| {}).await;
        let tether = user_ctx.user_stash().connection().await.unwrap();

        let original_conversation = conversations.into_iter().next().unwrap();
        let original_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        Message::action_mark_read(user_ctx.action_queue(), vec![original_messages[0].id()])
            .await
            .unwrap();
        let updated_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let updated_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(updated_messages.iter().filter(|m| m.unread).count(), 0);
        assert_eq!(updated_conversation.num_unread, 0);
        assert_eq!(updated_conversation.labels[0].context_num_unread, 0);

        user_ctx
            .core_context()
            .feature_flags()
            .test_override(MAIL_ET_REBASE_FEATURE_KEY, true)
            .await
            .unwrap();
        user_ctx
            .apply_event(
                MailEvent {
                    event_id: EventId::from("MyEvent"),
                    labels: None,
                    conversation_counts: None,
                    conversations: Some(vec![ConversationEvent {
                        id: original_conversation.remote_id.clone().unwrap(),
                        action: Action::Update,
                        conversation: Some(ApiConversation {
                            id: original_conversation.remote_id.clone().unwrap(),
                            attachment_info: Default::default(),
                            attachments_metadata: vec![],
                            display_snoozed_reminder: false,
                            expiration_time: original_conversation.expiration_time.as_u64(),
                            labels: original_conversation
                                .labels
                                .iter()
                                .cloned()
                                .map(|l| ApiConversationLabel {
                                    id: l.remote_label_id.unwrap(),
                                    context_expiration_time: l.context_expiration_time.as_u64(),
                                    context_num_attachments: l.context_num_attachments,
                                    context_num_messages: l.context_num_messages,
                                    context_num_unread: l.context_num_unread,
                                    context_size: l.context_size,
                                    context_snooze_time: l.context_snooze_time.as_u64(),
                                    context_time: l.context_time.as_u64(),
                                })
                                .collect(),
                            num_attachments: original_conversation.num_attachments,
                            num_messages: original_conversation.num_messages,
                            num_unread: original_conversation.num_unread,
                            order: 0,
                            recipients: vec![],
                            senders: vec![],
                            size: 0,
                            subject: "".to_string(),
                            context_time: None,
                        }),
                    }]),
                    incoming_defaults: None,
                    mail_settings: None,
                    message_counts: None,
                    messages: Some(vec![MessageEvent {
                        id: original_messages[0].remote_id.clone().unwrap(),
                        action: Action::Create,
                        message: Some(ApiMessageMetadata {
                            id: original_messages[0].remote_id.clone().unwrap(),
                            conversation_id: original_conversation.remote_id.clone().unwrap(),
                            address_id: original_messages[0].remote_address_id.clone(),
                            attachments_metadata: vec![],
                            bcc_list: vec![],
                            cc_list: vec![],
                            expiration_time: 0,
                            external_id: None,
                            flags: MessageFlags::empty(),
                            is_forwarded: false,
                            is_replied: false,
                            is_replied_all: false,
                            label_ids: original_messages[0].label_ids.clone(),
                            num_attachments: 0,
                            order: original_messages[0].display_order,
                            sender: Default::default(),
                            size: 0,
                            snooze_time: 0,
                            subject: "".to_string(),
                            time: original_messages[0].time.as_u64(),
                            to_list: vec![],
                            unread: original_messages[0].unread,
                        }),
                    }]),
                    refresh: 0,
                    has_more: false,
                }
                .into(),
            )
            .await
            .unwrap();

        let rebased_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let rebased_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(rebased_conversation, updated_conversation);
        assert_ne!(rebased_conversation, original_conversation);
        assert_eq!(rebased_messages, updated_messages);
    }

    #[tokio::test]
    async fn mark_read_rebase_to_target_state_still_invokes_server() {
        let (_test_ctx, user_ctx, conversations) =
            setup_with_mocks(vec![vec![true, false, true]], async |ctx, conv| {
                ctx.mock_mark_conversation_read(vec![conv[0].remote_id.clone().unwrap()], vec![])
                    .await
            })
            .await;

        let tether = user_ctx.user_stash().connection().await.unwrap();
        let original_conversation = conversations.into_iter().next().unwrap();
        let local_inbox_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap();

        Conversation::action_mark_read(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();

        let changeset = RebaseChangeSet::from(original_conversation.id());
        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn simple_mark_unread() {
        let (_test_ctx, user_ctx, conversations) =
            setup_with_mocks(vec![vec![false, false, false]], async |ctx, conv| {
                ctx.mock_mark_conversation_unread(
                    vec![conv[0].remote_id.clone().unwrap()],
                    LabelId::inbox(),
                    vec![],
                )
                .await
            })
            .await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();
        let local_inbox_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap();

        let mut original_conversation = conversations.into_iter().next().unwrap();
        let mut original_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        let queued = Conversation::action_mark_unread(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();
        let updated_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let updated_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(updated_messages.iter().filter(|m| m.unread).count(), 1);
        assert_eq!(updated_conversation.num_unread, 1);
        assert_eq!(updated_conversation.labels[0].context_num_unread, 1);

        // reset state
        tether
            .tx(async |tx| {
                for msg in &mut original_messages {
                    msg.save(tx).await.unwrap();
                }
                original_conversation.save(tx).await
            })
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_conversation.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let rebased_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(rebased_conversation, updated_conversation);
        assert_ne!(rebased_conversation, original_conversation);
        assert_eq!(rebased_messages, updated_messages);

        user_ctx.action_queue().cancel(queued.id).await.unwrap();
        let reverted_conversation = Conversation::find_by_id(original_conversation.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let reverted_messages = Message::in_conversation(
            original_conversation.id(),
            ConversationViewOptions::All,
            &tether,
        )
        .await
        .unwrap();

        assert_eq!(reverted_conversation, original_conversation);
        assert_eq!(reverted_messages, original_messages);

        Conversation::action_mark_unread(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn mark_unread_rebase_to_target_state_still_invokes_server() {
        let (_test_ctx, user_ctx, conversations) =
            setup_with_mocks(vec![vec![false, false, false]], async |ctx, conv| {
                ctx.mock_mark_conversation_unread(
                    vec![conv[0].remote_id.clone().unwrap()],
                    LabelId::inbox(),
                    vec![],
                )
                .await
            })
            .await;

        let tether = user_ctx.user_stash().connection().await.unwrap();
        let original_conversation = conversations.into_iter().next().unwrap();
        let local_inbox_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap();

        Conversation::action_mark_unread(
            user_ctx.action_queue(),
            local_inbox_id,
            vec![original_conversation.id()],
        )
        .await
        .unwrap();

        let changeset = RebaseChangeSet::from(original_conversation.id());
        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }
}
