use crate::datatypes::SystemLabelId;
use crate::test_utils::test_context::MailTestContext;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
    Label as ApiLabel,
};
use proton_core_api::services::proton::{
    AddressFlags, AddressSignedKeyList as ApiAddressSignedKeyList,
};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::datatypes::{LabelColor, LabelType, LocalLabelId};
use proton_core_common::models::Label;
use proton_core_common::models::ModelIdExtension;
use proton_crypto_account::keys::AddressKeys as CryptoAddressKeys;
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId};
use proton_mail_api::services::proton::response_data::{
    AttachmentMetadata, Conversation as ApiConversation, ConversationLabel as ApiConversationLabel,
    MessageMetadata, MessageRecipient as ApiMessageRecipient, MessageSender as ApiMessageSender,
};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub static MY_LABEL_ID1: LazyLock<LabelId> = LazyLock::new(|| LabelId::from("MyLabelID1"));
pub static MY_LABEL_ID2: LazyLock<LabelId> = LazyLock::new(|| LabelId::from("MyLabelID2"));
pub static MY_ATTACHMENT_ID: LazyLock<AttachmentId> =
    LazyLock::new(|| AttachmentId::from("MyAttachmentID1"));
pub static MY_CONVERSATION_ID: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::from("MyConversationID"));

#[macro_export]
macro_rules! conv_id {
    ($id:expr) => {{
        use proton_mail_api::services::proton::common::ConversationId;
        Some(ConversationId::from($id.to_string()).into())
    }};
}

#[macro_export]
macro_rules! lbl_id {
    ($id:expr) => {{
        use proton_core_api::services::proton::LabelId;
        Some(LabelId::from($id.to_string()).into())
    }};
}

#[macro_export]
macro_rules! msg_id {
    ($id:expr) => {{
        use proton_mail_api::services::proton::common::MessageId;
        Some(MessageId::from($id.to_string()).into())
    }};
}

#[macro_export]
macro_rules! label {
    ($($field:tt)*) => {{
        use proton_core_common::models::Label;

        Label {
            $($field)*,
            ..Label::test_default()
        }}
    };
}

#[macro_export]
macro_rules! api_label {
    ($($field:tt)*) => {{
        use proton_core_api::services::proton::{Label as ApiLabel};

        ApiLabel {
            $($field)*,
            ..ApiLabel::test_default()
        }
    }};
}

#[macro_export]
macro_rules! message {
    ($($field:tt)*) => {{
        use $crate::models::Message;

        Message {
            $($field)*,
            ..Message::test_default()
        }
    }};
}

#[macro_export]
macro_rules! api_message {
    ($($field:tt)*) => {{
        use proton_mail_api::services::proton::response_data::{Message as ApiMessage};

        ApiMessage {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! api_message_meta {
    ($($field:tt)*) => {{
        use proton_mail_api::services::proton::response_data::{MessageMetadata as ApiMessageMetadata};

        ApiMessageMetadata {
            $($field)*,
            ..ApiMessageMetadata::test_default()
        }
    }};
}

#[macro_export]
macro_rules! conversation {
    ($($field:tt)*) => {{
        use $crate::models::Conversation;

        Conversation {
            $($field)*,
            ..Conversation::test_default()
        }
    }};
}

#[macro_export]
macro_rules! conv_label {
    ($($field:tt)*) => {{
        use $crate::models::ConversationLabel;

        ConversationLabel {
            $($field)*,
            ..ConversationLabel::test_default()
        }
    }};
}

#[macro_export]
macro_rules! api_conversation {
    ($($field:tt)*) => {{
        use proton_mail_api::services::proton::response_data::{Conversation as ApiConversation};

        ApiConversation {
            $($field)*,
            ..ApiConversation::test_default()
        }
    }};
}

impl MailTestContext {
    #[must_use]
    pub fn get_test_labels(&self) -> Vec<ApiLabel> {
        let label1 = ApiLabel {
            id: "Label1".into(),
            name: "Label1".into(),
            ..ApiLabel::test_default()
        };
        let label2 = ApiLabel {
            id: "Label2".into(),
            name: "Label2".into(),
            ..ApiLabel::test_default()
        };
        let label3 = ApiLabel {
            id: "Label3".into(),
            name: "Label3".into(),
            ..ApiLabel::test_default()
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
            flags: AddressFlags::default(),
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
            ..ApiConversation::test_default()
        }]
    }

    #[must_use]
    pub fn get_test_msgs(&self) -> Vec<MessageMetadata> {
        let m1 = MessageMetadata {
            id: "Message1".into(),
            address_id: AddressId::from("Addr1"),
            label_ids: vec![LabelId::from("Label1"), LabelId::from("Label2")],
            ..MessageMetadata::test_default()
        };
        let m2 = MessageMetadata {
            id: "Message2".into(),
            address_id: AddressId::from("Addr2"),
            label_ids: vec![LabelId::from("Label2"), LabelId::from("Label3")],
            ..MessageMetadata::test_default()
        };
        vec![m1, m2]
    }
}

/// Can panic if the local conversation `id` is not set, the remote
/// `label_id` is not set, the local `label` can not be found or the query
/// failed.
///
pub async fn create_labels(tether: &mut Tether) -> Vec<LocalLabelId> {
    let mut labels = [test_label1(), test_label2()];
    tether
        .tx::<_, _, StashError>(async |tx| {
            for label in &mut labels {
                label.save(tx).await.expect("failed to create labels");
                assert!(
                    Label::find_by_remote_id(label.remote_id.clone().unwrap(), tx)
                        .await
                        .expect("failed to resolve label ids")
                        .unwrap()
                        .local_id
                        .is_some()
                );
            }
            Ok(())
        })
        .await
        .expect("failed to commit transaction");

    labels.into_iter().map(|l| l.id()).collect()
}

#[must_use]
pub fn test_label1() -> Label {
    label!(
        remote_id: Some(MY_LABEL_ID1.clone()),
        name: "MyLabel".to_owned(),
        color: LabelColor::black(),
        label_type: LabelType::Label
    )
}

#[must_use]
pub fn test_label2() -> Label {
    label!(
       remote_id: Some(MY_LABEL_ID2.clone()),
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
        senders: vec![ApiMessageSender {
            address: "hello@world.com".into(),
            name: "HelloWorld".into(),
            ..Default::default()
        }],
        recipients: vec![
            ApiMessageRecipient {
                address: "foo@bar.com".into(),
                name: "Foo".into(),
                ..Default::default()
            },
            ApiMessageRecipient {
                address: "Bar@bar.com".into(),
                name: "bar".into(),
                ..Default::default()
            },
        ],
        num_messages: 10,
        num_unread: 4,
        num_attachments: 7,
        expiration_time: 1024,
        size: 4909,
        labels: Vec::from_iter(labels),
        display_snoozed_reminder: false,
        attachments_metadata: Vec::from_iter(attachments),
        attachment_info: BTreeMap::default(),
        context_time: None,
    }
}
