use crate::test_context::MailTestContext;
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::AddressSignedKeyList as ApiAddressSignedKeyList;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata, Conversation as ApiConversation, ConversationLabel as ApiConversationLabel,
    Label as ApiLabel, MessageAddress as ApiMessageAddress, MessageMetadata,
};
use proton_core_common::datatypes::RemoteId;
use proton_core_common::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, Id, LabelId, LocalId,
};
use proton_core_common::models::{Address, ModelExtension};
use proton_crypto_account::keys::AddressKeys as CryptoAddressKeys;
use proton_mail_common::datatypes::{LabelColor, LabelType, SystemLabelId};
use proton_mail_common::models::Label;
use stash::orm::Model;
use stash::stash::{Interface, Tether};
use std::collections::BTreeMap;

lazy_static! {
    pub static ref MY_ADDRESS_ID: ApiRemoteId = ApiRemoteId::from("MyRemoteId");
    pub static ref MY_LABEL_ID1: ApiRemoteId = ApiRemoteId::from("MyLabelID1");
    pub static ref MY_LABEL_ID2: ApiRemoteId = ApiRemoteId::from("MyLabelID2");
    pub static ref MY_ATTACHMENT_ID: ApiRemoteId = ApiRemoteId::from("MyAttachmentID1");
    pub static ref MY_CONVERSATION_ID: ApiRemoteId = ApiRemoteId::from("MyConversationID");
}

/// Macro wrapping u64 into Option<LocalId> for easier model definition.
#[macro_export]
macro_rules! lid {
    ($id:expr) => {{
        use proton_core_common::datatypes::LocalId;
        Some(LocalId::from($id))
    }};
}

/// Macro wrapping &str into Option<RemoteId> for easier model definition.
/// Since it calls ``.into()`` on the `RemoteId`, it allows creation of Option<LabelId> as well.
#[macro_export]
macro_rules! rid {
    ($id:expr) => {{
        use proton_core_common::datatypes::RemoteId;
        Some(RemoteId::from($id).into())
    }};
}

#[macro_export]
macro_rules! label {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Label;

        Label {
            $($field)*,
            ..Default::default()
        }}
    };
}

#[macro_export]
macro_rules! api_label {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Label as ApiLabel};

        ApiLabel {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! message {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Message;

        Message {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_message {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Message as ApiMessage};

        ApiMessage {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_message_meta {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{MessageMetadata as ApiMessageMetadata};

        ApiMessageMetadata {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! conversation {
    ($($field:tt)*) => {{
        use proton_mail_common::models::Conversation;

        Conversation {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_conversation {
    ($($field:tt)*) => {{
        use proton_api_mail::services::proton::response_data::{Conversation as ApiConversation};

        ApiConversation {
            $($field)*,
            ..Default::default()
        }
    }};
}

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
            address_type: ApiAddressType::Original,
            catch_all: Default::default(),
            display_name: String::default(),
            domain_id: Some(String::default()),
            email: String::default(),
            keys: CryptoAddressKeys(vec![]),
            order: Default::default(),
            proton_mx: Default::default(),
            receive: Default::default(),
            send: Default::default(),
            signature: String::default(),
            signed_key_list: ApiAddressSignedKeyList::default(),
            status: ApiAddressStatus::Enabled,
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

/// # Panics
pub async fn remote_counterpart<T: Model>(id: LocalId, tx: &Tether) -> RemoteId {
    id.counterpart::<T, _>(tx).await.unwrap().unwrap()
}

#[allow(dead_code)]
/// # Panics
pub async fn local_counterpart<T: Model>(id: RemoteId, tx: &Tether) -> LocalId {
    id.counterpart::<T, _>(tx).await.unwrap().unwrap()
}

/// Can panic if the local conversation `id` is not set, the remote
/// `label_id` is not set, the local `label` can not be found or the query
/// failed.
///
/// # Panics
pub async fn create_labels(tx: &Tether) -> Vec<LocalId> {
    let mut labels = [test_label1(), test_label2()];
    for label in &mut labels {
        label.save_using(tx).await.expect("failed to create labels");
        assert!(
            Label::find_by_id(RemoteId::from(label.remote_id.clone().unwrap()), tx.stash())
                .await
                .expect("failed to resolve label ids")
                .unwrap()
                .local_id
                .is_some()
        );
    }
    labels.into_iter().map(|l| l.local_id.unwrap()).collect()
}

/// Can panic if the local conversation `id` is not set, the remote
/// `label_id` is not set, the local `label` can not be found.
///
/// # Panics
pub async fn create_address(core_tx: &Tether) -> Address {
    let mut address = test_address();
    address
        .save_using(core_tx)
        .await
        .expect("failed to create address");

    address
}

#[must_use]
pub fn test_address() -> Address {
    Address {
        local_id: None,
        remote_id: Some(MY_ADDRESS_ID.clone().into()),
        email: "hello@world".to_owned(),
        send: Default::default(),
        receive: Default::default(),
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: AddressKeys::default(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 0,
        },
        row_id: None,
        stash: None,
    }
}

#[must_use]
pub fn test_label1() -> Label {
    label!(
        remote_id: Some(MY_LABEL_ID1.clone().into()),
        name: "MyLabel".to_owned(),
        color: LabelColor::black(),
        label_type: LabelType::Label
    )
}

#[must_use]
pub fn test_label2() -> Label {
    label!(
       remote_id: Some(MY_LABEL_ID2.clone().into()),
       name: "MyFolder".to_owned(),
       color: LabelColor::black(),
       label_type: LabelType::Folder,
       notify: true,
       expanded: true,
       display_order: 1
    )
}

#[must_use]
pub fn test_starred_label() -> Label {
    label!(
       remote_id: Some(LabelId::starred().clone()),
       name: "Starred".to_owned(),
       path: Some("Starred".to_owned()),
       color: LabelColor::black(),
       label_type: LabelType::System,
       display_order: 2
    )
}

pub fn test_conversation(
    labels: impl IntoIterator<Item = ApiConversationLabel>,
    attachments: impl IntoIterator<Item = AttachmentMetadata>,
) -> ApiConversation {
    ApiConversation {
        id: MY_CONVERSATION_ID.clone(),
        order: 50,
        subject: "Hello World".to_owned(),
        senders: vec![ApiMessageAddress {
            address: "hello@world.com".to_owned(),
            name: "HelloWorld".to_owned(),
            ..Default::default()
        }],
        recipients: vec![
            ApiMessageAddress {
                address: "foo@bar.com".to_owned(),
                name: "Foo".to_owned(),
                ..Default::default()
            },
            ApiMessageAddress {
                address: "Bar@bar.com".to_owned(),
                name: "bar".to_owned(),
                ..Default::default()
            },
        ],
        num_messages: 10,
        num_unread: 4,
        num_attachments: 7,
        expiration_time: 1024,
        size: 4909,
        labels: Vec::from_iter(labels),
        display_snooze_reminder: false,
        attachments_metadata: Vec::from_iter(attachments),
        attachment_info: BTreeMap::default(),
    }
}
