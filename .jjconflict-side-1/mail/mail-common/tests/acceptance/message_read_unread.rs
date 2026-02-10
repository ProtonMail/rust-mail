use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::MailSettings as ApiMailSettings;
use proton_mail_api::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_mail_api::services::proton::response_data::ViewMode as ApiViewMode;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;
use test_case::test_case;

struct TestCase {
    id: &'static str,
    to_mark: bool,
    unread: bool,
}

static EMPTY: &[TestCase] = &[];

static ALL_UNREAD: &[TestCase] = &[
    TestCase {
        id: "one",
        to_mark: true,
        unread: true,
    },
    TestCase {
        id: "two",
        to_mark: true,
        unread: true,
    },
    TestCase {
        id: "three",
        to_mark: false,
        unread: false,
    },
    TestCase {
        id: "four",
        to_mark: false,
        unread: true,
    },
];

static MIXED_UNREAD: &[TestCase] = &[
    TestCase {
        id: "one",
        to_mark: true,
        unread: true,
    },
    TestCase {
        id: "two",
        to_mark: true,
        unread: false,
    },
    TestCase {
        id: "three",
        to_mark: false,
        unread: false,
    },
    TestCase {
        id: "four",
        to_mark: false,
        unread: true,
    },
];

static ALL_READ: &[TestCase] = &[
    TestCase {
        id: "one",
        to_mark: true,
        unread: false,
    },
    TestCase {
        id: "two",
        to_mark: true,
        unread: false,
    },
    TestCase {
        id: "three",
        to_mark: false,
        unread: false,
    },
    TestCase {
        id: "four",
        to_mark: false,
        unread: true,
    },
];

#[test_case(&EMPTY, 0; "empty")]
#[test_case(&ALL_UNREAD, 1; "all unread")]
#[test_case(&MIXED_UNREAD, 1; "mixed unread")]
#[test_case(&ALL_READ, 1; "all read")]
#[tokio::test]
async fn mark_message_read(messages: &[TestCase], expected_unread: usize) {
    // Setup
    // * Create all given messages in stash
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

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
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();

    let messages = messages.iter().map(test_message(&params)).collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(messages.clone()).await;

    if !expected_to_mark.is_empty() {
        ctx.mock_put_messages_read(expected_to_mark, vec![]).await;
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

    if !messages.is_empty() {
        let mut conversation =
            Conversation::find_by_remote_id(params.conversations[0].id.clone(), &tether)
                .await
                .unwrap()
                .unwrap();
        conversation.num_unread = messages.len() as u64;
        tether
            .tx(async |bond| conversation.save(bond).await)
            .await
            .unwrap();
    }

    // Action
    let message_ids = Message::remote_ids_counterpart(to_mark, &tether)
        .await
        .unwrap();
    Message::action_mark_read(user_ctx.action_queue(), message_ids)
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

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
async fn mark_message_unread(messages: &[TestCase], expected_unread: usize) {
    // Setup
    // * Create all given messages in stash
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

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
        .filter(|m| m.to_mark)
        .map(|m| m.id.into())
        .collect_vec();
    let messages = messages.iter().map(test_message(&params)).collect_vec();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(messages).await;

    if !expected_to_mark.is_empty() {
        ctx.mock_put_messages_unread(expected_to_mark, vec![]).await;
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

    // Action
    let message_ids = Message::remote_ids_counterpart(to_mark, &tether)
        .await
        .unwrap();
    Message::action_mark_unread(user_ctx.action_queue(), message_ids)
        .await
        .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    // Validation
    let messages = Message::find("WHERE unread = ?", params![true], &tether)
        .await
        .unwrap();
    assert_eq!(messages.len(), expected_unread);
}

fn test_message(params: &Params) -> impl FnMut(&TestCase) -> ApiMessageMetadata {
    let conversation_id = params.conversations[0].id.clone();
    let address_id = params.addresses[0].id.clone();
    move |message| {
        let TestCase {
            id: name, unread, ..
        } = message;
        ApiMessageMetadata {
            id: MessageId::from(name.to_owned()),
            conversation_id: conversation_id.clone(),
            address_id: address_id.clone(),
            unread: *unread,
            ..ApiMessageMetadata::test_default()
        }
    }
}

mod rebase {
    use super::*;
    use proton_action_queue::action::ActionGroup;
    use proton_action_queue::rebase::RebaseChangeSet;
    use proton_core_api::services::proton::AddressId;
    use proton_core_common::models::{Address, ModelExtension};
    use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
    use proton_mail_api::services::proton::common::ConversationId;
    use proton_mail_common::MailUserContext;
    use stash::stash::StashError;
    use std::sync::Arc;

    async fn setup(unread: Vec<bool>) -> (MailTestContext, Arc<MailUserContext>, Vec<Message>) {
        setup_with_mocks(unread, async |_, _| {}).await
    }
    async fn setup_with_mocks(
        unread: Vec<bool>,
        mk_mocks: impl AsyncFnOnce(&MailTestContext, &[Message]),
    ) -> (MailTestContext, Arc<MailUserContext>, Vec<Message>) {
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

        let mut messages = unread
            .into_iter()
            .enumerate()
            .map(|(idx, unread)| Message {
                remote_id: Some(MessageId::from(format!("msg-{idx}"))),
                remote_conversation_id: Some(ConversationId::from(format!("conv-{idx}"))),
                remote_address_id: addr_id.clone(),
                local_address_id: local_addr_id,
                unread,
                time: ((idx * 100) as u64).into(),
                ..Message::test_default()
            })
            .collect_vec();

        tether
            .tx::<_, _, StashError>(async |tx| {
                for message in &mut messages {
                    message.save(tx).await?;
                    message.reload(tx).await?;
                }
                Ok(())
            })
            .await
            .unwrap();

        mk_mocks(&ctx, &messages).await;

        (ctx, user_ctx, messages)
    }

    #[tokio::test]
    async fn simple_mark_read() {
        let (_test_ctx, user_ctx, messages) = setup_with_mocks(vec![true], async |ctx, msgs| {
            ctx.mock_put_messages_read(vec![msgs[0].remote_id.clone().unwrap()], vec![])
                .await
        })
        .await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();

        let mut original_message = messages.into_iter().next().unwrap();

        let queued =
            Message::action_mark_read(user_ctx.action_queue(), vec![original_message.id()])
                .await
                .unwrap();
        let updated_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        // reset state
        tether
            .tx(async |tx| original_message.save(tx).await)
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(rebased_message, updated_message);
        assert_ne!(rebased_message, original_message);

        user_ctx.action_queue().cancel(queued.id).await.unwrap();
        let reverted_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reverted_message, original_message);

        Message::action_mark_read(user_ctx.action_queue(), vec![original_message.id()])
            .await
            .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn rebase_mark_read_only_modifies_changed() {
        let (_test_ctx, user_ctx, messages) = setup(vec![true, true]).await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();
        let mut iter = messages.into_iter();
        let mut original_message1 = iter.next().unwrap();
        let mut original_message2 = iter.next().unwrap();

        Message::action_mark_read(
            user_ctx.action_queue(),
            vec![original_message1.id(), original_message2.id()],
        )
        .await
        .unwrap();
        let updated_message = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        // reset state
        tether
            .tx(async |tx| {
                original_message1.save(tx).await?;
                original_message2.save(tx).await
            })
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message1.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_message1 = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let rebased_message2 = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(rebased_message1, updated_message);
        assert_ne!(rebased_message2, original_message2);
    }

    #[tokio::test]
    async fn rebase_to_target_state_mark_read_still_invokes_server() {
        let (_test_ctx, user_ctx, messages) = setup_with_mocks(vec![true], async |ctx, msgs| {
            ctx.mock_put_messages_read(vec![msgs[0].remote_id.clone().unwrap()], vec![])
                .await
        })
        .await;

        let original_message = messages.into_iter().next().unwrap();
        Message::action_mark_read(user_ctx.action_queue(), vec![original_message.id()])
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn simple_mark_unread() {
        let (_test_ctx, user_ctx, messages) = setup_with_mocks(vec![false], async |ctx, msgs| {
            ctx.mock_put_messages_unread(vec![msgs[0].remote_id.clone().unwrap()], vec![])
                .await
        })
        .await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();

        let mut original_message = messages.into_iter().next().unwrap();

        let queued =
            Message::action_mark_unread(user_ctx.action_queue(), vec![original_message.id()])
                .await
                .unwrap();
        let updated_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        // reset state
        tether
            .tx(async |tx| original_message.save(tx).await)
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(rebased_message, updated_message);
        assert_ne!(rebased_message, original_message);

        user_ctx.action_queue().cancel(queued.id).await.unwrap();
        let reverted_message = Message::find_by_id(original_message.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reverted_message, original_message);

        Message::action_mark_unread(user_ctx.action_queue(), vec![original_message.id()])
            .await
            .unwrap();

        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn rebase_to_target_state_mark_unread_still_invokes_server() {
        let (_test_ctx, user_ctx, messages) = setup_with_mocks(vec![false], async |ctx, msgs| {
            ctx.mock_put_messages_unread(vec![msgs[0].remote_id.clone().unwrap()], vec![])
                .await
        })
        .await;

        let original_message = messages.into_iter().next().unwrap();
        Message::action_mark_unread(user_ctx.action_queue(), vec![original_message.id()])
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        assert_eq!(user_ctx.execute_all_actions().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn rebase_mark_unread_only_modifies_changed() {
        let (_test_ctx, user_ctx, messages) = setup(vec![false, false]).await;
        let mut tether = user_ctx.user_stash().connection().await.unwrap();
        let mut iter = messages.into_iter();
        let mut original_message1 = iter.next().unwrap();
        let mut original_message2 = iter.next().unwrap();

        Message::action_mark_unread(
            user_ctx.action_queue(),
            vec![original_message1.id(), original_message2.id()],
        )
        .await
        .unwrap();
        let updated_message = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        // reset state
        tether
            .tx(async |tx| {
                original_message1.save(tx).await?;
                original_message2.save(tx).await
            })
            .await
            .unwrap();
        let changeset = RebaseChangeSet::from(original_message1.id());

        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &changeset)
            .await
            .unwrap();
        let rebased_message1 = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();
        let rebased_message2 = Message::find_by_id(original_message1.id(), &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(rebased_message1, updated_message);
        assert_ne!(rebased_message2, original_message2);
    }
}
