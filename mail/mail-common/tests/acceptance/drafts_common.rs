#![allow(dead_code)]
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_inbox::attachment::KeyPackets;
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_api::services::proton::prelude::{ContentDisposition, MailSettings};
use proton_mail_api::services::proton::request_data::{DraftParams, DraftRecipient, DraftSender};
use proton_mail_api::services::proton::response_data::AttachmentMetadata;
use proton_mail_api::services::proton::response_data::{
    Disposition, Message as ApiMessage, MessageAttachment, MessageAttachmentHeaders,
};
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::draft::ReplyMode;
use proton_mail_common::draft::compose::{DEFAULT_SUBJECT, FORWARD_PREFIX, REPLY_PREFIX};
use proton_mail_common::draft::recipients::{RecipientEntry, RecipientList};
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::message_body::{
    message_body_test_addresses, message_body_test_mail_settings, message_body_test_message_simple,
    message_body_test_user_info,
};

const MOCK_ATTACHMENT_KEY_PACKET: &str = "wV4DGS71hsmM2EQSAQdAwLggNHWQfw7ZdO/BJrT4WpD3yK2ZhqRt6/abVcoKii4wVlG50hY+UgSoVOf3RBJ33bastQrBMK25JsRJqFByq2t2BXKojQVQtP9B1CmjNjZ0";

pub fn draft_test_params() -> TestParams {
    draft_test_params_impl(None)
}
pub fn draft_test_params_with_mime_type(mime_type: MimeType) -> TestParams {
    draft_test_params_impl(Some(mime_type))
}
pub fn draft_test_params_impl(mime_type: Option<MimeType>) -> TestParams {
    let mut mail_settings = message_body_test_mail_settings();
    if let Some(mime_type) = mime_type {
        mail_settings.draft_mime_type = mime_type.into();
    }
    let mut params = TestParams {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(mail_settings),
        ..Default::default()
    };

    params.addresses[0].signature = "Sent from rust rest".to_owned();

    // Add another address to check if the empty draft grabs the
    // correct primary address. Using this key will result in a crypto
    // error.
    params.addresses.push(ApiAddress {
        id: AddressId::from("GIBBERISH TEST ID"),
        email: "gibberish@proton.ch".to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 2,
        display_name: "gibberish".to_owned(),
        signature: "".to_owned(),
        keys: ApiAddressKeys(vec![LockedKey {
            id: KeyId::from("GIBBERISH"),
            version: 3,
            private_key: ArmoredPrivateKey::from("GIBBERISH".to_owned()),
            token: Some(EncryptedKeyToken::from("GIBBERISH".to_owned())),
            signature: Some(KeyTokenSignature::from("GIBBERISH".to_owned())),
            activation: None,
            primary: true,
            active: true,
            flags: Some(KeyFlag::from(3_u32)),
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]),
        catch_all: false,
        proton_mx: true,
        signed_key_list: ApiAddressSignedKeyList {
            min_epoch_id: Some(3),
            max_epoch_id: Some(66),
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: Some("GIBBERISH".to_owned()),
            revision: 1,
        },
    });
    params
}

pub fn expected_create_draft_params() -> DraftParams {
    let address = message_body_test_addresses();
    DraftParams {
        subject: DEFAULT_SUBJECT.to_owned(),
        unread: false,
        sender: DraftSender {
            address: address[0].email.clone().into(),
            name: address[0].display_name.clone().into(),
        },
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        external_id: None,
        draft_flags: 0,
        body: EncryptedDraft(String::new()),
        mime_type: MailSettings::default().draft_mime_type,
    }
}
pub fn expected_create_reply_draft_params(
    message: &Message,
    mime_type: MimeType,
    reply_mode: ReplyMode,
) -> DraftParams {
    let address = message_body_test_addresses();
    let mut params = DraftParams {
        subject: format!(
            "{}{}",
            if reply_mode == ReplyMode::Forward {
                FORWARD_PREFIX
            } else {
                REPLY_PREFIX
            },
            message.subject
        ),
        unread: false,
        sender: DraftSender {
            address: address[0].email.clone().into(),
            name: address[0].display_name.clone().into(),
        },
        to_list: vec![DraftRecipient {
            address: message.sender.address.clone(),
            name: message.sender.name.clone(),
            group: None,
        }],
        cc_list: vec![],
        bcc_list: vec![],
        external_id: None,
        draft_flags: 0,
        body: EncryptedDraft(String::new()),
        mime_type: mime_type.into(),
    };

    if reply_mode == ReplyMode::Forward {
        params.to_list.clear();
        params.cc_list.clear();
    }

    params
}

pub fn draft_message_with_attachments() -> ApiMessage {
    let mut remote_existing_message = draft_message();
    let normal_attchment = gen_normal_attachment();
    remote_existing_message.body.attachments =
        vec![gen_inline_attachment(), normal_attchment.clone()];

    remote_existing_message
        .metadata
        .attachments_metadata
        .push(AttachmentMetadata {
            id: normal_attchment.id,
            disposition: normal_attchment.disposition,
            mime_type: normal_attchment.mime_type,
            name: normal_attchment.name,
            size: normal_attchment.size,
        });

    remote_existing_message
}

pub fn gen_inline_attachment() -> MessageAttachment {
    MessageAttachment {
        id: "MyInlineAttachment".into(),
        disposition: Disposition::Inline,
        enc_signature: None,
        headers: MessageAttachmentHeaders {
            content_disposition: ContentDisposition::One("inline".to_owned()),
            content_id: Some("InlineCID".to_owned()),
            content_transfer_encoding: None,
            image_height: None,
            image_width: None,
        },
        key_packets: KeyPackets::from(MOCK_ATTACHMENT_KEY_PACKET),
        mime_type: "image/jpeg".to_owned(),
        name: "image.jpeg".to_owned(),
        signature: None,
        size: 123,
    }
}
pub fn gen_normal_attachment() -> MessageAttachment {
    MessageAttachment {
        id: "MyAttachment".into(),
        disposition: Disposition::Attachment,
        enc_signature: None,
        headers: MessageAttachmentHeaders {
            content_disposition: ContentDisposition::One("attachment".to_owned()),
            content_id: None,
            content_transfer_encoding: None,
            image_height: None,
            image_width: None,
        },
        key_packets: KeyPackets::from(MOCK_ATTACHMENT_KEY_PACKET),
        mime_type: "application/pdf".to_owned(),
        name: "doc.pdf".to_owned(),
        signature: None,
        size: 1024,
    }
}

pub fn new_recipient_list_with_single_address(email: String) -> RecipientList {
    let mut list = RecipientList::new();
    list.add_single(RecipientEntry {
        email: email.into(),
        name: None,
    })
    .unwrap();
    list
}

pub fn draft_message() -> ApiMessage {
    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.extend([
        LabelId::all_drafts(),
        LabelId::drafts(),
        LabelId::all_mail(),
    ]);
    message
}
