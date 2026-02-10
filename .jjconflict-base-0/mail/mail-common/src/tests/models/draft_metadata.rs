use crate as proton_mail_common;
use crate::models::DraftMetadata;
use proton_action_queue::action::ActionDependencyKeys;
use proton_action_queue::db::StoredAction;
use proton_core_common::actions::event_poll::EventPoll;
use proton_core_common::models::ModelExtension;
use proton_mail_common::test_utils::db::new_test_connection;
use proton_mail_common::test_utils::utils::create_address;
use proton_mail_common::{conv_id, conversation, message, msg_id};
use stash::orm::Model;
use stash::stash::StashError;

#[tokio::test]
async fn test_messages_with_pending_send() {
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let address = create_address(&mut tether).await;
    tether
        .tx::<_, _, StashError>(async |bond| {
            let conversation = conversation!(remote_id: conv_id!(1))
                .with_save(bond)
                .await
                .unwrap();
            let message_1 = message!(
                remote_id: msg_id!(1),
                local_conversation_id: conversation.local_id,
                remote_conversation_id: conv_id!(1),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.clone().unwrap()
            )
            .with_save(bond)
            .await
            .unwrap();
            let message_2 = message!(
                remote_id: msg_id!(2),
                local_conversation_id: conversation.local_id,
                remote_conversation_id: conv_id!(1),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.unwrap()
            )
            .with_save(bond)
            .await
            .unwrap();
            let action_1 = StoredAction::without_state::<EventPoll>(
                ActionDependencyKeys::default(),
                Default::default(),
            )
            .with_save(bond)
            .await
            .unwrap();
            let action_2 = StoredAction::without_state::<EventPoll>(
                ActionDependencyKeys::default(),
                Default::default(),
            )
            .with_save(bond)
            .await
            .unwrap();

            DraftMetadata::builder()
                .build()
                .with_save(bond)
                .await
                .unwrap();

            DraftMetadata::builder()
                .local_message_id(message_1.id())
                .build()
                .with_save(bond)
                .await
                .unwrap();

            DraftMetadata::builder()
                .send_action_id(action_1.id.unwrap())
                .build()
                .with_save(bond)
                .await
                .unwrap();

            // Only this one should be returned in `messages_with_pending_send`
            DraftMetadata::builder()
                .local_message_id(message_2.id())
                .send_action_id(action_2.id.unwrap())
                .build()
                .with_save(bond)
                .await
                .unwrap();

            assert_eq!(DraftMetadata::count("", vec![], bond).await.unwrap(), 4);
            assert_eq!(
                DraftMetadata::messages_with_pending_send(bond)
                    .await
                    .unwrap(),
                vec![message_2.id()]
            );
            Ok(())
        })
        .await
        .unwrap();
}
