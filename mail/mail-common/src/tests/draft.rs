pub use super::*;
use proton_core_common::datatypes::{AddressStatus, AddressType};

#[test]
fn new_draft_message_creation() {
    let address = address_with_signature("");
    let display_order = 512_u64;
    let body_len = 4098_usize;

    let message = create_new_message(&address, display_order, body_len);
    assert_eq!(message.display_order, display_order);
    assert_eq!(message.size, body_len as u64);
    assert_eq!(message.subject, DEFAULT_SUBJECT);
    assert_eq!(message.local_address_id, address.local_id.unwrap());
    assert_eq!(message.remote_address_id, address.remote_id.unwrap());
    assert_eq!(message.sender.address, address.email);
    assert_eq!(message.sender.name, address.display_name);
    assert!(message.to_list.value.is_empty());
    assert!(message.cc_list.value.is_empty());
    assert!(message.bcc_list.value.is_empty());
}

#[test]
fn reply_draft_message_creation() {
    //TODO: Check attachments (ET-1362)
    let address = address_with_signature("");
    let mut source_message = existing_message();

    let display_order = 512_u64;
    let body_len = 4098_usize;
    let draft = create_new_draft_with_reply_mode(
        &mut source_message,
        ReplyMode::Sender,
        &address,
        display_order,
        body_len,
    );

    assert_eq!(
        draft.local_conversation_id,
        source_message.local_conversation_id
    );
    assert_eq!(
        draft.remote_conversation_id,
        source_message.remote_conversation_id
    );
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject)
    );
    assert_eq!(draft.sender.address, address.email);
    assert_eq!(draft.sender.name, address.display_name);
    assert_eq!(draft.to_list.value, vec![source_message.sender]);
    assert!(draft.cc_list.value.is_empty());
    assert!(draft.bcc_list.value.is_empty());
    assert!(source_message.is_replied);
    assert!(!(source_message.flags & MessageFlags::REPLIED).is_empty());
    assert_eq!(draft.display_order, display_order);
    assert_eq!(draft.size, body_len as u64);
}

#[test]
fn reply_all_draft_message_creation() {
    //TODO: Check attachments (ET-1362)
    let address = address_with_signature("");
    let mut source_message = existing_message();

    let display_order = 512_u64;
    let body_len = 4098_usize;
    let draft = create_new_draft_with_reply_mode(
        &mut source_message,
        ReplyMode::All,
        &address,
        display_order,
        body_len,
    );

    assert_eq!(
        draft.local_conversation_id,
        source_message.local_conversation_id
    );
    assert_eq!(
        draft.remote_conversation_id,
        source_message.remote_conversation_id
    );
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject)
    );
    assert_eq!(draft.sender.address, address.email);
    assert_eq!(draft.sender.name, address.display_name);
    assert_eq!(draft.to_list.value, vec![source_message.sender]);
    assert_eq!(draft.cc_list.value, source_message.cc_list.value);
    assert!(draft.bcc_list.value.is_empty());
    assert!(source_message.is_replied_all);
    assert!(!(source_message.flags & MessageFlags::REPLIED_ALL).is_empty());
    assert_eq!(draft.display_order, display_order);
    assert_eq!(draft.size, body_len as u64);
}

#[test]
fn forward_draft_message_creation() {
    //TODO: Check attachments (ET-1362)
    let address = address_with_signature("");
    let mut source_message = existing_message();

    let display_order = 512_u64;
    let body_len = 4098_usize;
    let draft = create_new_draft_with_reply_mode(
        &mut source_message,
        ReplyMode::Forward,
        &address,
        display_order,
        body_len,
    );

    assert_eq!(
        draft.local_conversation_id,
        source_message.local_conversation_id
    );
    assert_eq!(
        draft.remote_conversation_id,
        source_message.remote_conversation_id
    );
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject)
    );
    assert_eq!(draft.sender.address, address.email);
    assert_eq!(draft.sender.name, address.display_name);
    assert!(draft.to_list.value.is_empty());
    assert!(draft.cc_list.value.is_empty());
    assert!(draft.bcc_list.value.is_empty());
    assert!(source_message.is_forwarded);
    assert!(!(source_message.flags & MessageFlags::FORWARDED).is_empty());
    assert_eq!(draft.display_order, display_order);
    assert_eq!(draft.size, body_len as u64);
}

#[test]
fn message_signature_empty_without_address_or_mail_setting_signature() {
    let address = address_with_signature("");
    let mail_settings = MailSettings::default();
    let signature = get_signature(&address, &mail_settings);
    assert!(signature.is_empty());
}

#[test]
fn message_signature_with_signature_only() {
    let address = address_with_signature(ADDRESS_SIGNATURE);
    let mail_settings = MailSettings::default();
    let signature = get_signature(&address, &mail_settings);
    assert_eq!(signature, format!("\n\n{ADDRESS_SIGNATURE}"));
}

#[test]
fn message_signature_with_mail_settings_signature_only() {
    let address = address_with_signature("");
    let mail_settings = mail_settings_with_signature();
    let signature = get_signature(&address, &mail_settings);
    assert_eq!(signature, format!("\n\n{MAIL_SETTINGS_SIGNATURE}"));
}

#[test]
fn message_signature_with_address_and_mail_settings_signature() {
    let address = address_with_signature(ADDRESS_SIGNATURE);
    let mail_settings = mail_settings_with_signature();
    let signature = get_signature(&address, &mail_settings);
    assert_eq!(
        signature,
        format!("\n\n{ADDRESS_SIGNATURE}\n\n{MAIL_SETTINGS_SIGNATURE}")
    );
}

fn address_with_signature(signature: impl Into<String>) -> Address {
    Address {
        local_id: Some(local_address_id()),
        remote_id: Some(remote_address_id()),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "Addr Display Name".to_owned(),
        display_order: 0,
        domain_id: None,
        email: "address_email@proton.me".to_owned(),
        keys: Default::default(),
        proton_mx: false,
        receive: false,
        send: false,
        signature: signature.into(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        row_id: None,
        stash: None,
    }
}

fn mail_settings_with_signature() -> MailSettings {
    let mut settings = MailSettings::default();
    settings.signature = MAIL_SETTINGS_SIGNATURE.to_owned();
    settings
}

fn existing_message() -> Message {
    Message {
        local_id: None,
        remote_id: None,
        local_conversation_id: Some(local_conversation_id()),
        remote_conversation_id: Some(remote_conversation_id()),
        local_address_id: local_address_id(),
        remote_address_id: remote_address_id(),
        attachments_metadata: vec![],
        cc_list: MessageAddresses {
            value: vec![MessageAddress {
                address: "cc_contact_1@pm.me".to_string(),
                bimi_selector: None,
                display_sender_image: true,
                is_proton: false,
                is_simple_login: true,
                name: "CC Contact".to_string(),
            }],
        },
        bcc_list: Default::default(),
        deleted: false,
        exclusive_location: None,
        expiration_time: 0,
        external_id: None,
        flags: Default::default(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![],
        num_attachments: 0,
        display_order: 0,
        reply_tos: Default::default(),
        sender: MessageAddress {
            address: "sender@void.org".to_owned(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: "Send InToVoid".to_string(),
        },
        size: 0,
        snooze_time: 0,
        subject: "".to_string(),
        time: 0,
        to_list: Default::default(),
        unread: false,
        custom_labels: vec![],
        cached: false,
        row_id: None,
        stash: None,
    }
}

fn local_conversation_id() -> LocalId {
    11111111.into()
}

fn remote_conversation_id() -> RemoteId {
    RemoteId::new("My remote conv id".to_owned())
}

fn local_address_id() -> LocalId {
    9000.into()
}

fn remote_address_id() -> RemoteId {
    RemoteId::new("My remote addr id".to_owned())
}
const ADDRESS_SIGNATURE: &str = "My Address Signature";
const MAIL_SETTINGS_SIGNATURE: &str = "Mail settings signature";
