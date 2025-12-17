use itertools::Itertools;
use proton_action_queue::action::ActionGroup;
use proton_action_queue::queue::QueuedError;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Address, Label, ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{ConversationCounters, LabelExt, Message, MessageCounters};
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::sync::Arc;

#[tokio::test]
async fn smoke() {
    let (_test_ctx, user_ctx, _) = setup(5, async |ctx, _| {
        ctx.mock_empty_label(LabelId::inbox()).await;
    })
    .await;

    let queue = user_ctx.action_queue();
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let label = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let mut msg_counter = msg_counter(&label, &mut tether).await;
    let mut conv_counter = conv_counter(&label, &mut tether).await;

    // ---

    assert!(label.is_idle(&tether).await.unwrap());

    Message::action_delete_all_in_label(queue, label.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(0, msg_counter.total);
    assert_eq!(0, msg_counter.unread);
    assert_eq!(0, conv_counter.total);
    assert_eq!(0, conv_counter.unread);

    // ---

    assert!(label.is_busy(&tether).await.unwrap());

    // Make sure we don't allow to schedule parallel delete-alls - see the
    // action's constructor for more details.
    assert!(
        Message::action_delete_all_in_label(queue, label.id(), &tether)
            .await
            .unwrap()
            .is_none()
    );

    // ---

    assert!(
        Message::in_label(label.id(), &tether)
            .await
            .unwrap()
            .is_empty()
    );

    user_ctx.execute_single_action().await.unwrap();
}

#[tokio::test]
async fn revert() {
    let (_test_ctx, user_ctx, _) = setup(5, async |_, _| {}).await;

    let queue = user_ctx.action_queue();
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let label = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let mut msg_counter = msg_counter(&label, &mut tether).await;
    let mut conv_counter = conv_counter(&label, &mut tether).await;

    // ---

    assert!(label.is_idle(&tether).await.unwrap());

    Message::action_delete_all_in_label(queue, label.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(0, msg_counter.total);
    assert_eq!(0, msg_counter.unread);
    assert_eq!(0, conv_counter.total);
    assert_eq!(0, conv_counter.unread);

    // ---

    let err = user_ctx.execute_single_action().await.unwrap_err();

    // 404 Not Found
    assert!(matches!(err, QueuedError::Action(_, _)));

    // ---

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(10, msg_counter.total);
    assert_eq!(8, msg_counter.unread);
    assert_eq!(3, conv_counter.total);
    assert_eq!(2, conv_counter.unread);
}

#[tokio::test]
async fn rebase() {
    let (_test_ctx, user_ctx, messages) = setup(5, async |_, _| {}).await;

    let new_messages = messages
        .iter()
        .cloned()
        .enumerate()
        .map(|(idx, m)| Message {
            local_id: None,
            remote_id: Some(MessageId::from(format!("msg-{}", 10 + idx))),
            time: ((30 + idx) as u64).into(),
            ..m
        })
        .collect::<Vec<_>>();

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let label = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let mut msg_counter = msg_counter(&label, &mut tether).await;
    let mut conv_counter = conv_counter(&label, &mut tether).await;

    // ---

    let queued = Message::action_delete_all_in_label(user_ctx.action_queue(), label.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(
        0,
        Message::in_label(label.id(), &tether).await.unwrap().len(),
    );

    assert_eq!(0, msg_counter.total);
    assert_eq!(0, msg_counter.unread);
    assert_eq!(0, conv_counter.total);
    assert_eq!(0, conv_counter.unread);

    // ---

    tether
        .tx(async |tx| {
            MessageCounters {
                local_label_id: label.id(),
                total: new_messages.len() as u64,
                unread: new_messages.len() as u64,
            }
            .save(tx)
            .await?;

            for mut msg in new_messages {
                msg.save(tx).await?;
            }

            Ok::<_, StashError>(())
        })
        .await
        .unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(
        5,
        Message::in_label(label.id(), &tether).await.unwrap().len(),
    );

    assert_eq!(5, msg_counter.total);
    assert_eq!(5, msg_counter.unread);
    assert_eq!(0, conv_counter.total);
    assert_eq!(0, conv_counter.unread);

    // ---

    user_ctx
        .action_queue()
        .rebase(ActionGroup::default(), &RebaseChangeSet::default())
        .await
        .unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(
        0,
        Message::in_label(label.id(), &tether).await.unwrap().len(),
    );

    assert_eq!(0, msg_counter.total);
    assert_eq!(0, msg_counter.unread);
    assert_eq!(0, conv_counter.total);
    assert_eq!(0, conv_counter.unread);

    // ---

    user_ctx.action_queue().cancel(queued.id).await.unwrap();

    msg_counter.reload(&tether).await.unwrap();
    conv_counter.reload(&tether).await.unwrap();

    assert_eq!(
        10,
        Message::in_label(label.id(), &tether).await.unwrap().len(),
    );

    assert_eq!(10, msg_counter.total);
    assert_eq!(8, msg_counter.unread);
    assert_eq!(3, conv_counter.total);
    assert_eq!(2, conv_counter.unread);
}

async fn setup(
    count: usize,
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

    let mut messages = (0..count)
        .map(|idx| Message {
            remote_id: Some(MessageId::from(format!("msg-{idx}"))),
            remote_conversation_id: Some(ConversationId::from(format!("conv-{idx}"))),
            remote_address_id: addr_id.clone(),
            local_address_id: local_addr_id,
            label_ids: vec![LabelId::inbox()],
            time: ((idx * 10) as u64).into(),
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

async fn msg_counter(label: &Label, tether: &mut Tether) -> MessageCounters {
    let mut counter = MessageCounters {
        local_label_id: label.id(),
        total: 10,
        unread: 8,
    };

    tether.tx(async |tx| counter.save(tx).await).await.unwrap();

    counter
}

async fn conv_counter(label: &Label, tether: &mut Tether) -> ConversationCounters {
    let mut counter = ConversationCounters {
        local_label_id: label.id(),
        total: 3,
        unread: 2,
    };

    tether.tx(async |tx| counter.save(tx).await).await.unwrap();

    counter
}
