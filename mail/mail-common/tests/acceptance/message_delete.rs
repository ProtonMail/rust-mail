use itertools::Itertools;
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::models::{Address, Label, ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::account::TEST_ADDRESS_ID;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::stash::StashError;
use std::sync::Arc;

#[tokio::test]
async fn smoke() {
    let (_test_ctx, user_ctx, _) = setup(5, async |ctx, _| {
        ctx.mock_empty_label(LabelId::inbox()).await;
    })
    .await;

    let tether = user_ctx.user_stash().connection().await.unwrap();

    let local_label_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
        .await
        .unwrap()
        .unwrap();

    Message::action_delete_all_in_label(user_ctx.action_queue(), local_label_id, &tether)
        .await
        .unwrap();

    let messages = Message::in_label(local_label_id, &tether).await.unwrap();
    assert!(messages.is_empty());

    user_ctx.execute_single_action().await.unwrap();
}

mod rebase {
    use super::*;
    use proton_action_queue::action::ActionGroup;
    use proton_action_queue::rebase::RebaseChangeSet;

    #[tokio::test]
    async fn rebase_reapplies_to_all() {
        let (_test_ctx, user_ctx, messages) = setup(5, async |_, _| {}).await;

        let mut new_messages = messages
            .iter()
            .cloned()
            .enumerate()
            .map(|(idx, m)| Message {
                local_id: None,
                remote_id: Some(MessageId::from(format!("msg-{}", 100 + idx))),
                time: ((300 + idx) as u64).into(),
                ..m
            })
            .collect::<Vec<_>>();

        let mut tether = user_ctx.user_stash().connection().await.unwrap();

        let local_label_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap();

        let queued =
            Message::action_delete_all_in_label(user_ctx.action_queue(), local_label_id, &tether)
                .await
                .unwrap();

        assert_eq!(
            Message::in_label(local_label_id, &tether)
                .await
                .unwrap()
                .len(),
            0,
        );

        tether
            .tx(async |tx| {
                for msg in &mut new_messages {
                    msg.save(tx).await?;
                }
                Ok::<_, StashError>(())
            })
            .await
            .unwrap();

        assert_eq!(
            Message::in_label(local_label_id, &tether)
                .await
                .unwrap()
                .len(),
            5
        );
        user_ctx
            .action_queue()
            .rebase(ActionGroup::default(), &RebaseChangeSet::default())
            .await
            .unwrap();

        assert_eq!(
            Message::in_label(local_label_id, &tether)
                .await
                .unwrap()
                .len(),
            0
        );

        user_ctx.action_queue().cancel(queued.id).await.unwrap();

        assert_eq!(
            Message::in_label(local_label_id, &tether)
                .await
                .unwrap()
                .len(),
            10
        );
    }
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
