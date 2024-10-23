use crate::test_context::MailTestContext;
use proton_api_core::services::proton::response_data::AddressSignedKeyList;
use proton_api_core::services::proton::{
    common::RemoteId as ApiRemoteId,
    response_data::{Address as ApiAddress, AddressStatus, AddressType},
};
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel as ApiConversationLabel, Label as ApiLabel,
    MessageMetadata,
};
use proton_crypto_account::keys::AddressKeys;

impl MailTestContext {
    #[must_use]
    pub fn get_test_labels(&self) -> Vec<ApiLabel> {
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

    #[must_use]
    pub fn default_address(&self) -> ApiAddress {
        ApiAddress {
            id: "".into(),
            address_type: AddressType::Original,
            catch_all: Default::default(),
            display_name: String::default(),
            domain_id: Some(String::default()),
            email: String::default(),
            keys: AddressKeys(vec![]),
            order: Default::default(),
            proton_mx: Default::default(),
            receive: Default::default(),
            send: Default::default(),
            signature: String::default(),
            signed_key_list: AddressSignedKeyList::default(),
            status: AddressStatus::Enabled,
        }
    }

    #[must_use]
    pub fn get_test_addrs(&self) -> Vec<ApiAddress> {
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

    #[must_use]
    pub fn default_conv_label(&self) -> ApiConversationLabel {
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

    #[must_use]
    pub fn get_test_convers(&self) -> Vec<ApiConversation> {
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

    #[must_use]
    pub fn get_test_msgs(&self) -> Vec<MessageMetadata> {
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
