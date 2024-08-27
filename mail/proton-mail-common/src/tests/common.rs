#![allow(non_snake_case)]

use crate::datatypes::{LabelColor, LabelType, SystemLabelId};
use crate::models::Label;
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, Conversation as ApiConversation,
    ConversationLabel as ApiConversationLabel, MessageAddress as ApiMessageAddress,
};
use proton_core_common::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, LabelId, LocalId, RemoteId,
};
use proton_core_common::models::{Address, ModelExtension};
use stash::stash::{Interface, Tether};

lazy_static! {
    pub static ref MY_ADDRESS_ID: ApiRemoteId = ApiRemoteId::from("MyRemoteId");
    pub static ref MY_LABEL_ID1: ApiRemoteId = ApiRemoteId::from("MyLabelID1");
    pub static ref MY_LABEL_ID2: ApiRemoteId = ApiRemoteId::from("MyLabelID2");
    pub static ref MY_ATTACHMENT_ID: ApiRemoteId = ApiRemoteId::from("MyAttachmentID1");
    pub static ref MY_CONVERSATION_ID: ApiRemoteId = ApiRemoteId::from("MyConversationID");
}

#[macro_export]
macro_rules! label {
    ($($field:tt)*) => {
        Label {
            $($field)*,
            ..Default::default()
        }
    };
}

#[macro_export]
macro_rules! message {
    ($($field:tt)*) => {{
        Message {
            $($field)*,
            ..Default::default()
        }
    }};
}

#[macro_export]
macro_rules! conversation {
    ($($field:tt)*) => {{
        Conversation {
            $($field)*,
            ..Default::default()
        }
    }};
}

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

pub async fn create_address(core_tx: &Tether) -> Address {
    let mut address = test_address();
    address
        .save_using(core_tx)
        .await
        .expect("failed to create address");

    address
}

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

pub fn test_label1() -> Label {
    label!(
        remote_id: Some(MY_LABEL_ID1.clone().into()),
        name: "MyLabel".to_owned(),
        color: LabelColor::black(),
        label_type: LabelType::Label
    )
}

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
    attachments: impl IntoIterator<Item = ApiAttachmentMetadata>,
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
        attachment_info: Default::default(),
    }
}
