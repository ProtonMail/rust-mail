use common::TestContext;
use proton_api_core::{
    services::proton::{
        common::RemoteId as ApiRemoteId,
        response_data::{Address as ApiAddress, AddressStatus, AddressType},
    },
    session::CoreSession,
};
use proton_api_mail::services::proton::{
    requests::{GetConversationsOptions, GetMessagesOptions},
    response_data::{
        Conversation as ApiConversation, ConversationLabel as ApiConversationLabel,
        Label as ApiLabel, MessageMetadata,
    },
};
use proton_core_common::{
    datatypes::RemoteId,
    models::{Address, ModelExtension},
};
use proton_crypto_account::keys::AddressKeys;
use proton_mail_common::models::{Conversation, Label, Message};

mod common;

impl TestContext {
    fn get_test_labels(&self) -> Vec<ApiLabel> {
        let label1 = ApiLabel {
            id: "Label1".into(),
            name: "Label1".into(),
            ..Default::default()
        };
        let label2 = ApiLabel {
            id: "Label2".into(),
            name: "Label2".into(),
            ..Default::default()
        };
        let label3 = ApiLabel {
            id: "Label3".into(),
            name: "Label3".into(),
            ..Default::default()
        };

        vec![label1, label2, label3]
    }

    fn default_address(&self) -> ApiAddress {
        ApiAddress {
            id: "".into(),
            address_type: AddressType::Original,
            catch_all: Default::default(),
            display_name: Default::default(),
            domain_id: Default::default(),
            email: Default::default(),
            keys: AddressKeys(vec![]),
            order: Default::default(),
            proton_mx: Default::default(),
            receive: Default::default(),
            send: Default::default(),
            signature: Default::default(),
            signed_key_list: Default::default(),
            status: AddressStatus::Enabled,
        }
    }

    fn get_test_addrs(&self) -> Vec<ApiAddress> {
        let addr1 = ApiAddress {
            id: "Addr1".into(),
            email: "foo@bar".into(),
            ..self.default_address()
        };
        let addr2 = ApiAddress {
            id: "Addr2".into(),
            email: "foo@baz".into(),
            ..self.default_address()
        };

        vec![addr1, addr2]
    }

    fn default_conv_label(&self) -> ApiConversationLabel {
        ApiConversationLabel {
            id: "".into(),
            context_expiration_time: Default::default(),
            context_num_attachments: Default::default(),
            context_num_messages: Default::default(),
            context_num_unread: Default::default(),
            context_size: Default::default(),
            context_snooze_time: Default::default(),
            context_time: Default::default(),
        }
    }

    fn get_test_convers(&self) -> Vec<ApiConversation> {
        vec![ApiConversation {
            id: "Conv1".into(),
            labels: vec![
                ApiConversationLabel {
                    id: "Label1".into(),
                    ..self.default_conv_label()
                },
                ApiConversationLabel {
                    id: "Label2".into(),
                    ..self.default_conv_label()
                },
                ApiConversationLabel {
                    id: "Label3".into(),
                    ..self.default_conv_label()
                },
            ],
            ..Default::default()
        }]
    }

    fn get_test_msgs(&self) -> Vec<MessageMetadata> {
        let m1 = MessageMetadata {
            id: "Message1".into(),
            address_id: ApiRemoteId::from("Addr1"),
            label_ids: vec![ApiRemoteId::from("Label1"), ApiRemoteId::from("Label2")],
            ..Default::default()
        };
        let m2 = MessageMetadata {
            id: "Message2".into(),
            address_id: ApiRemoteId::from("Addr2"),
            label_ids: vec![ApiRemoteId::from("Label2"), ApiRemoteId::from("Label3")],
            ..Default::default()
        };
        vec![m1, m2]
    }
}

#[tokio::test]
async fn unsynced_conversations() {
    let ctx = TestContext::new().await;
    let user_context = ctx.user_context().await;
    let stash = user_context.user_stash();
    let api = user_context.session().api();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;
    ctx.mock_get_conversations(ctx.get_test_convers(), 1).await;

    let options = GetConversationsOptions::default();
    Conversation::search(options, api, stash)
        .await
        .expect("Error searching for conversations");

    // Now all of the labels should exist!
    Label::find_by_id(RemoteId::from("Label1"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label2"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label3"), stash)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn unsynced_messages() {
    let ctx = TestContext::new().await;
    let user_context = ctx.user_context().await;
    let stash = user_context.user_stash();
    let api = user_context.session().api();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;
    ctx.mock_get_all_addresses(ctx.get_test_addrs()).await;
    ctx.mock_get_messages(ctx.get_test_msgs()).await;

    let options = GetMessagesOptions::default();
    Message::search(options, api, stash)
        .await
        .expect("Error searching for messages");

    // Now all of the labels should exist!
    Label::find_by_id(RemoteId::from("Label1"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label2"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label3"), stash)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_id(RemoteId::from("Addr1"), stash)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_id(RemoteId::from("Addr2"), stash)
        .await
        .unwrap()
        .unwrap();
}
