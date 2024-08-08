#![allow(non_snake_case)]

use super::super::*;
use crate::db::new_test_connection_file;
use crate::AppError;
use proton_api_mail::services::proton::response_data::{
    Attachment as ApiAttachment, AttachmentMetadata as ApiAttachmentMetadata,
    Disposition as ApiDisposition, MessageAddress as ApiMessageAddress,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    MimeType as ApiMimeType,
};
use proton_core_common::datatypes::{AddressKeys, AddressStatus, AddressType, RemoteId};
use proton_core_common::models::Address;
use proton_crypto_account::keys::AddressKeys as RealAddressKeys;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use stash::orm::Model;
use stash::stash::Tether;

#[tokio::test]
async fn test_attachment_create_without_metadata() {
    // Simulates an attachment's full info being stored without having any previous
    // message or conversation metadata.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let (_, _, _) = create_attachment_dependencies(&tx, None).await.unwrap();
    let api_attachment = test_attachment();
    let mut attachment = Attachment::from(api_attachment.clone());
    attachment.save_using(&tx).await.unwrap();
    let local_id = attachment.local_id;
    assert!(attachment.has_complete_metadata());
    let mut expected = Attachment::from(api_attachment);
    expected.local_id = local_id;
    expected.row_id = attachment.row_id;
    expected.set_stash(&stash);
    let db_attachment = Attachment::load(local_id.unwrap(), &stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(expected, db_attachment);
}

#[tokio::test]
async fn test_attachment_create_with_metadata() {
    // Simulates an attachment's full info being stored with an existing
    // message or conversation metadata.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let api_attachment = test_attachment();
    let metadata = ApiAttachmentMetadata {
        id: api_attachment.id.clone(),
        size: api_attachment.size,
        name: api_attachment.name.clone(),
        mime_type: api_attachment.mime_type,
        disposition: api_attachment.disposition,
    };
    let (_, _, _) = create_attachment_dependencies(&tx, Some(metadata))
        .await
        .unwrap();

    let db_attachment = Attachment::load(1, &stash).await.unwrap().unwrap();
    assert!(!db_attachment.has_complete_metadata());

    let mut attachment = Attachment::from(api_attachment.clone());
    attachment.save_or_update(&tx.into()).await.unwrap();
    let local_id = attachment.local_id;
    assert!(attachment.has_complete_metadata());
    let mut expected = attachment.clone();
    expected.local_id = local_id;
    expected.row_id = attachment.row_id;
    expected.stash = Some(stash.clone());
    let db_attachment = Attachment::load(local_id.unwrap(), &stash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(expected, db_attachment);
}
fn test_attachment() -> ApiAttachment {
    ApiAttachment {
        id: RemoteId::from("attachment").into(),
        name: "attachment_foo".to_owned(),
        size: 1024,
        mime_type: ApiMimeType::TextPlain,
        disposition: ApiDisposition::Inline,
        key_packets: RealKeyPackets::from("key_packets"),
        signature: Some(RealAttachmentSignature::from("signature")),
        enc_signature: Some(RealAttachmentEncryptedSignature::from("enc_signature")),
        sender: Some(ApiMessageAddress {
            address: "fooo".to_owned(),
            name: "fooo".to_owned(),
            is_proton: false,
            display_sender_image: true,
            is_simple_login: false,
            bimi_selector: None,
        }),
        address_id: address_id().into(),
        message_id: message_id().into(),
        conversation_id: conversation_id().into(),
        is_auto_forwardee: true,
    }
}

fn address_id() -> RemoteId {
    RemoteId::from("addr")
}
fn conversation_id() -> RemoteId {
    RemoteId::from("conv")
}
fn message_id() -> RemoteId {
    RemoteId::from("msg")
}

async fn create_attachment_dependencies(
    tx: &Tether,
    metadata: Option<ApiAttachmentMetadata>,
) -> Result<(RemoteId, u64, u64), AppError> {
    let metadata = metadata.map(|v| vec![v]).unwrap_or_default();

    Address {
        remote_id: Some(address_id()),
        email: String::new(),
        send: false,
        receive: false,
        status: AddressStatus::Disabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_order: 0,
        display_name: String::new(),
        signature: String::new(),
        keys: AddressKeys::from(RealAddressKeys::new(vec![])),
        catch_all: false,
        proton_mx: false,
        signed_key_list: Default::default(),
        row_id: None,
        stash: None,
    }
    .save_using(tx)
    .await?;

    let local_conv_ids = Conversation::create_or_update_conversations(
        vec![Conversation {
            remote_id: Some(conversation_id()),
            attachments_metadata: metadata.clone().into_iter().map(|m| m.into()).collect(),
            ..Default::default()
        }],
        tx.stash(),
    )
    .await?;

    let local_msg_ids = Message::create_or_update_messages_from_metadata(
        vec![ApiMessageMetadata {
            id: message_id().into(),
            conversation_id: conversation_id().into(),
            order: 0,
            address_id: address_id().into(),
            label_ids: vec![],
            external_id: None,
            subject: String::new(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: ApiMessageFlags::empty(),
            time: 0,
            size: 0,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: metadata.clone(),
        }],
        tx.stash(),
    )
    .await?;

    Ok((
        address_id(),
        *local_conv_ids.first().unwrap(),
        *local_msg_ids.first().unwrap(),
    ))
}
