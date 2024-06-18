use crate::db::{u64, u64, u64, with_file_sqlite_db, Attachment};
use crate::exports::crypto::keys::AddressKeys;
use proton_api_mail::domain::{
    Attachment, AttachmentId, AttachmentMetadata, Conversation, ConversationId, Disposition,
    MessageAddress, MessageFlags, MessageId, MessageMetadata,
};
use proton_api_mail::proton_api_core::domain::{Address, AddressId, AddressStatus, AddressType};
use proton_crypto_inbox::attachment::{
    AttachmentEncryptedSignature, AttachmentSignature, KeyPackets,
};
use stash::stash::StashError;

#[test]
fn test_attachment_create_without_metadata() {
    // Simulates an attachment's full info being stored without having any previous
    // message or conversation metadata.
    with_file_sqlite_db(|mut core_conn, mut mail_conn, _| {
        let (_, conv_id, message_id) =
            create_attachment_dependencies(&mut core_conn, &mut mail_conn, None).unwrap();
        let tx = mail_conn.transaction().await.unwrap();
        let attachment = test_attachment();
        let local_id = tx.create_or_update_attachment(&attachment)?;
        assert!(
            tx.is_attachment_metadata_complete(local_id)
                .unwrap()
                .unwrap()
                .0
        );
        let expected =
            Attachment::from_attachment(local_id, conv_id, Some(message_id), &attachment);
        let db_attachment = tx.attachment_with_id(local_id).unwrap().unwrap();
        assert_eq!(expected, db_attachment);
        Ok(())
    })
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn test_attachment_create_with_metadata() {
    // Simulates an attachment's full info being stored with an existing
    // message or conversation metadata.
    with_file_sqlite_db(|mut core_conn, mut mail_conn, _| {
        let attachment = test_attachment();
        let metadata = AttachmentMetadata {
            id: attachment.id.clone(),
            size: attachment.size,
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            disposition: attachment.disposition.clone(),
        };
        let (_, conv_id, message_id) =
            create_attachment_dependencies(&mut core_conn, &mut mail_conn, Some(metadata)).unwrap();
        let tx = mail_conn.transaction().await.unwrap();
        assert!(
            !tx.is_attachment_metadata_complete(u64::new(1))
                .unwrap()
                .unwrap()
                .0
        );
        let local_id = tx.create_or_update_attachment(&attachment)?;
        assert!(
            tx.is_attachment_metadata_complete(local_id)
                .unwrap()
                .unwrap()
                .0
        );
        let expected =
            Attachment::from_attachment(local_id, conv_id, Some(message_id), &attachment);
        let db_attachment = tx.attachment_with_id(local_id).unwrap().unwrap();
        assert_eq!(expected, db_attachment);
        Ok(())
    })
    .unwrap();
    tx.commit().await.unwrap();
}
fn test_attachment() -> Attachment {
    Attachment {
        id: AttachmentId::from("attachment"),
        name: "attachment_foo".to_string(),
        size: 1024,
        mime_type: "foo/bar".to_string(),
        disposition: Disposition::Inline,
        key_packets: KeyPackets::from("key_packets"),
        signature: Some(AttachmentSignature::from("signature")),
        enc_signature: Some(AttachmentEncryptedSignature::from("enc_signature")),
        sender: Some(MessageAddress {
            address: "fooo".to_string(),
            name: "fooo".to_string(),
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

fn create_attachment_dependencies(
    core_conn: &mut CoreSqliteConnection,
    conn: &mut MailSqliteConnection,
    metadata: Option<AttachmentMetadata>,
) -> Result<(AddressId, u64, u64), StashError> {
    let metadata = metadata.map(|v| vec![v]).unwrap_or_default();
    let addr_id = address_id();
    let conv_id = conversation_id();
    let msg_id = message_id();

    core_conn.tx(|tx| {
        tx.create_or_update_address(&Address {
            id: addr_id.clone(),
            email: "".to_string(),
            send: false,
            receive: false,
            status: AddressStatus::Disabled,
            domain_id: None,
            address_type: AddressType::Original,
            order: 0,
            display_name: "".to_string(),
            signature: "".to_string(),
            keys: AddressKeys(vec![]),
            catch_all: false,
            proton_mx: false,
            signed_key_list: Default::default(),
        })
    })?;

    conn.tx(move |tx| {
        let local_conv_id = tx.create_conversation(&Conversation {
            id: conv_id.clone(),
            order: 0,
            subject: "".to_string(),
            senders: vec![],
            recipients: vec![],
            num_messages: 0,
            num_unread: 0,
            num_attachments: 0,
            expiration_time: 0,
            size: 0,
            labels: vec![],
            display_snooze_reminder: false,
            attachments_metadata: metadata.clone(),
            attachment_info: Default::default(),
        })?;

        let local_msg_id = tx.create_message_from_metadata(&MessageMetadata {
            id: msg_id.clone(),
            conversation_id: conv_id.clone(),
            order: 0,
            address_id: addr_id.clone(),
            label_ids: vec![],
            external_id: None,
            subject: "".to_string(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: MessageFlags::empty(),
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
        })?;

        Ok((addr_id.clone(), local_conv_id, local_msg_id))
    })
}
