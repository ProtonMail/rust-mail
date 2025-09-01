use super::*;
use crate::AppError;
use proton_core_common::datatypes::{AddressKeys, AddressStatus, AddressType};
use proton_core_common::models::Address;
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature as RealAttachmentEncryptedSignature,
    AttachmentSignature as RealAttachmentSignature, KeyPackets as RealKeyPackets,
};
use proton_mail_api::services::proton::response_data::{
    Attachment as ApiAttachment, AttachmentMetadata as ApiAttachmentMetadata,
    Disposition as ApiDisposition, MessageFlags as ApiMessageFlags,
    MessageMetadata as ApiMessageMetadata, MessageSender as ApiMessageSender,
};
use proton_mail_common::test_utils::db::new_test_connection_file;
use stash::orm::Model;

#[tokio::test]
async fn test_attachment_create_without_metadata() {
    // Simulates an attachment's full info being stored without having any previous
    // message or conversation metadata.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let (_, _, _) = create_attachment_dependencies(&mut conn, None)
        .await
        .unwrap();
    let api_attachment = test_attachment();
    let mut attachment = Attachment::from(api_attachment.clone());
    conn.tx(async |tx| attachment.save(tx).await).await.unwrap();
    let local_id = attachment.id();
    assert!(attachment.has_complete_metadata());
    let mut expected = Attachment::from(api_attachment);
    expected.local_address_id = Some(1.into());
    expected.local_id = Some(local_id);
    expected.local_message_id = Some(1.into());
    expected.local_conversation_id = Some(1.into());
    let db_attachment = Attachment::load(local_id, &conn).await.unwrap().unwrap();
    assert_eq!(expected, db_attachment);
}

#[tokio::test]
async fn test_attachment_create_with_metadata() {
    // Simulates an attachment's full info being stored with an existing
    // message or conversation metadata.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let api_attachment = test_attachment();
    let metadata = ApiAttachmentMetadata {
        id: api_attachment.id.clone(),
        size: api_attachment.size,
        name: api_attachment.name.clone(),
        mime_type: api_attachment.mime_type.clone(),
        disposition: api_attachment.disposition,
    };
    let (_, _, _) = create_attachment_dependencies(&mut conn, Some(metadata))
        .await
        .unwrap();

    let db_attachment = Attachment::load(1.into(), &conn).await.unwrap().unwrap();
    assert!(!db_attachment.has_complete_metadata());

    let mut attachment = Attachment::from(api_attachment.clone());
    conn.tx(async |tx| attachment.save(tx).await).await.unwrap();
    let local_id = attachment.local_id;
    assert!(attachment.has_complete_metadata());
    let mut expected = attachment.clone();
    expected.local_id = local_id;
    let db_attachment = Attachment::load(local_id.unwrap(), &conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(expected, db_attachment);
}
fn test_attachment() -> ApiAttachment {
    ApiAttachment {
        id: AttachmentId::from("attachment"),
        name: "attachment_foo".to_owned(),
        size: 1024,
        mime_type: attachment::MimeType::text_plain().to_string(),
        disposition: ApiDisposition::Inline,
        key_packets: RealKeyPackets::from("key_packets"),
        signature: Some(RealAttachmentSignature::from("signature")),
        enc_signature: Some(RealAttachmentEncryptedSignature::from("enc_signature")),
        sender: Some(ApiMessageSender {
            address: "fooo".into(),
            name: "fooo".into(),
            is_proton: false,
            display_sender_image: true,
            is_simple_login: false,
            bimi_selector: None,
        }),
        address_id: address_id(),
        message_id: message_id(),
        conversation_id: conversation_id(),
        is_auto_forwardee: true,
    }
}

fn address_id() -> AddressId {
    AddressId::from("addr")
}
fn conversation_id() -> ConversationId {
    ConversationId::from("conv")
}
fn message_id() -> MessageId {
    MessageId::from("msg")
}

async fn create_attachment_dependencies(
    tether: &mut Tether,
    metadata: Option<ApiAttachmentMetadata>,
) -> Result<(AddressId, LocalConversationId, LocalMessageId), AppError> {
    let metadata = metadata.map(|v| vec![v]).unwrap_or_default();
    tether
        .tx(async |tx| {
            Address {
                local_id: None,
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
                keys: AddressKeys::default(),
                catch_all: false,
                proton_mx: false,
                signed_key_list: Default::default(),
            }
            .save(tx)
            .await?;

            let local_conv_ids = Conversation::create_or_update_conversations(
                vec![Conversation {
                    remote_id: Some(conversation_id()),
                    attachments_metadata: metadata
                        .clone()
                        .into_iter()
                        .map(AttachmentMetadata::from)
                        .collect(),
                    ..Conversation::test_default()
                }],
                tx,
            )
            .await?;

            let local_msg_ids = Message::create_or_update_messages_from_metadata(
                vec![ApiMessageMetadata {
                    id: message_id(),
                    conversation_id: conversation_id(),
                    order: 0,
                    address_id: address_id(),
                    label_ids: vec![],
                    external_id: None,
                    subject: String::new(),
                    sender: Default::default(),
                    to_list: vec![],
                    cc_list: vec![],
                    bcc_list: vec![],
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
                tx,
            )
            .await?;
            Ok((
                address_id(),
                *local_conv_ids.first().unwrap(),
                *local_msg_ids.first().unwrap(),
            ))
        })
        .await
}
