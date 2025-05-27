pub use super::*;
use crate::datatypes::LocalAttachmentId;
use crate::datatypes::LocalConversationId;
use crate::datatypes::LocalMessageId;
use crate::datatypes::attachment;
use crate::datatypes::{Disposition, MessageRecipient, MessageRecipients, MessageSender};
use crate::decrypted_message::DecryptedMessageBody;
use crate::draft::recipients::{MaybeEmptyString, NullContactGroupResolver};
use crate::draft::{Draft, MetadataId};
use crate::models::{Attachment, MessageBodyMetadata};
use crate::proton_mail_api::services::proton::prelude::ConversationId;
use insta::assert_snapshot;
use proton_core_common::datatypes::{AddressStatus, AddressType, LocalAddressId};
use std::str::FromStr;

#[test]
fn new_draft_message_creation() {
    let address = address_with_signature("");
    let mail_settings = MailSettings::default();
    let draft = Draft::new_empty_draft(MetadataId(0), &address, &mail_settings);

    assert!(draft.subject.is_empty());
    assert_eq!(draft.address_id, address.remote_id.unwrap());
    assert_eq!(draft.sender, address.email);
    assert!(draft.to_list.is_empty());
    assert!(draft.cc_list.is_empty());
    assert!(draft.bcc_list.is_empty());
}

#[tokio::test]
async fn reply_draft_message_creation() {
    let (draft, source_message, attachments) = create_reply(ReplyMode::Sender).await;
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject)
    );
    assert!(draft.to_list.contains_email(&source_message.sender.address));
    assert!(draft.cc_list.is_empty());
    assert!(draft.bcc_list.is_empty());
    assert_eq!(attachments, vec![inline_attachment()])
}

#[tokio::test]
async fn reply_all_draft_message_creation() {
    let (draft, source_message, attachments) = create_reply(ReplyMode::All).await;
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject)
    );
    assert!(draft.to_list.contains_email(&source_message.sender.address));
    assert!(
        draft
            .cc_list
            .contains_emails(source_message.cc_list.value.into_iter().map(|v| v.address))
    );
    assert!(draft.bcc_list.is_empty());
    assert_eq!(attachments, vec![inline_attachment()])
}

#[tokio::test]
async fn check_reply_signature_html() {
    let (draft, _, _) = create_reply_with(ReplyMode::All, MimeType::TextHtml).await;
    assert_snapshot!(draft.body());
}

#[tokio::test]
async fn check_reply_signature_text() {
    let mut source_body_metadata = existing_message_body_metadata();
    source_body_metadata.mime_type = MimeType::TextPlain;
    let source_body = "Hello World".to_owned();
    let (draft, _, _) = create_reply_with_mime_and_body(
        ReplyMode::All,
        MimeType::TextPlain,
        source_body_metadata,
        source_body,
    )
    .await;
    assert_snapshot!(draft.body());
}

#[tokio::test]
async fn check_reply_from_html_with_text_as_default() {
    let (draft, _, _) = create_reply_with(ReplyMode::All, MimeType::TextPlain).await;
    assert_snapshot!(draft.body());
    assert_eq!(draft.mime_type, MimeType::TextHtml);
}

#[tokio::test]
async fn forward_draft_message_creation() {
    let (draft, source_message, attachments) = create_reply(ReplyMode::Forward).await;
    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject)
    );
    assert!(draft.to_list.is_empty());
    assert!(draft.cc_list.is_empty());
    assert!(draft.bcc_list.is_empty());
    assert_eq!(attachments, vec![inline_attachment(), normal_attachment()])
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
    insta::assert_snapshot!(signature);
}

#[test]
fn message_signature_with_mail_settings_signature_only() {
    // mail settings signature should not be rendered as it is deprecated.
    let address = address_with_signature("");
    let mail_settings = mail_settings_with_signature();
    let signature = get_signature(&address, &mail_settings);
    insta::assert_snapshot!(signature);
}

#[test]
fn message_signature_with_address_and_mail_settings_signature() {
    // mail settings signature should not be rendered as it is deprecated.
    let address = address_with_signature(ADDRESS_SIGNATURE);
    let mail_settings = mail_settings_with_signature();
    let signature = get_signature(&address, &mail_settings);
    insta::assert_snapshot!(signature);
}

#[test]
fn message_signature_with_all_signatures() {
    // mail settings signature should not be rendered as it is deprecated.
    let address = address_with_signature(ADDRESS_SIGNATURE);
    let mail_settings = mail_settings_with_signature_and_pm_signautre();
    let signature = get_signature(&address, &mail_settings);
    insta::assert_snapshot!(signature);
}

#[tokio::test]
async fn sanitize_draft_reply_html() {
    // Draft replies need to be sanitized.
    let (mut draft, _, _) = create_reply_with_mime_and_body(
        ReplyMode::All,
        MimeType::TextHtml,
        sanitize_message_body_metadata(MimeType::TextHtml),
        DRAFT_BODY_HTML.to_owned(),
    )
    .await;

    assert_snapshot!(draft.body);

    let sanitized = draft.body.clone();

    draft.sanitize_body();
    assert_snapshot!(draft.body());

    // This should be identical before the save.
    assert_eq!(sanitized, draft.body);
}

fn sanitize_message_body_metadata(mime_type: MimeType) -> MessageBodyMetadata {
    MessageBodyMetadata {
        mime_type,
        ..Default::default()
    }
}

const DRAFT_BODY_HTML: &str = r##"
<html>
<head>
<style>
body {
    color: red;
}
</style>
</head>
<body>
<section>
    <svg id="svigi" width="5cm" height="4cm" version="1.1"
    xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <image x="0" y="0" height="50px" width="50px" xlink:href="firefox.jpg" />
        <image x="0" y="0" height="50px" width="50px" xlink:href="chrome.jpg" />
        <image x="0" y="0" height="50px" width="50px" href="svg-href.jpg" />
    </svg>
    <div>
        <img border="0" usemap="#fp" src="cats.jpg ">
        <map name="fp">
            <area coords="0,0,800,800" href="proton_exploit.html" shape="rect" target="_blank" >
        </map>
    </div>

    <img width="" height="" alt="" src="mon-image.jpg" srcset="mon-imageHD.jpg 2x">
    <img width="" height="" alt="" src="lol-image.jpg" srcset="lol-imageHD.jpg 2x">
    <img width="" height="" alt="" data-src="lol-image.jpg">
    <a href="lol-image.jpg">Alll</a>
    <a href="jeanne-image.jpg">Alll</a>
    <div background="jeanne-image.jpg">Alll</div>
    <div background="jeanne-image2.jpg">Alll</div>
    <p style="font-size:10.0pt;font-family:\\2018Calibri\\2019;color:black">
        Example style that caused regexps to crash
    </p>
    <img id="babase64" src="data:image/jpg;base64,iVBORw0KGgoAAAANSUhEUgAABoIAAAVSCAYAAAAisOk2AAAMS2lDQ1BJQ0MgUHJv
ZmlsZQAASImVVwdYU8kWnltSSWiBUKSE3kQp0qWE0CIISBVshCSQUGJMCCJ2FlkF
1y4ioK7oqoiLrgWQtaKudVHs/aGIysq6WLCh8iYF1tXvvfe9831z758z5/ynZO69
MwDo1PKk0jxUF4B8SYEsITKUNTEtnUXqAgSgD1AwGozk8eVSdnx8DIAydP+nvLkO"
    />
</section>
</body>
<html>
"##;

async fn create_reply(reply_mode: ReplyMode) -> (Draft, Message, Vec<Attachment>) {
    create_reply_with(reply_mode, MimeType::default()).await
}

async fn create_reply_with(
    reply_mode: ReplyMode,
    mime_type: MimeType,
) -> (Draft, Message, Vec<Attachment>) {
    let source_body_metadata = existing_message_body_metadata();
    let source_body = "Hello World".to_owned();
    create_reply_with_mime_and_body(reply_mode, mime_type, source_body_metadata, source_body).await
}

async fn create_reply_with_mime_and_body(
    reply_mode: ReplyMode,
    mime_type: MimeType,
    source_body_metadata: MessageBodyMetadata,
    source_body: String,
) -> (Draft, Message, Vec<Attachment>) {
    let source_message = existing_message();
    let address = address_with_signature("");
    let mail_settings = MailSettings {
        draft_mime_type: mime_type,
        ..MailSettings::default()
    };
    let source_body = DecryptedMessageBody {
        body: source_body,
        metadata: source_body_metadata,
        pgp_subject: None,
        in_flight: Default::default(),
    };

    let resolver = NullContactGroupResolver {};
    let (draft, attachments) = Draft::new_draft_reply(
        &resolver,
        MetadataId(0),
        reply_mode,
        &address,
        &mail_settings,
        &source_message,
        source_body,
        true,
        None,
    )
    .await;
    (draft, source_message, attachments)
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
    }
}

fn mail_settings_with_signature() -> MailSettings {
    MailSettings {
        signature: MAIL_SETTINGS_SIGNATURE.to_owned(),
        ..Default::default()
    }
}

fn mail_settings_with_signature_and_pm_signautre() -> MailSettings {
    MailSettings {
        signature: MAIL_SETTINGS_SIGNATURE.to_owned(),
        pm_signature: PmSignature::Enabled,
        ..Default::default()
    }
}

fn existing_message() -> Message {
    Message {
        local_id: Some(local_msg_id()),
        remote_id: None,
        local_conversation_id: Some(local_conversation_id()),
        remote_conversation_id: Some(remote_conversation_id()),
        local_address_id: local_address_id(),
        remote_address_id: remote_address_id(),
        attachments_metadata: vec![],
        cc_list: MessageRecipients {
            value: vec![MessageRecipient {
                address: "cc_contact_1@pm.me".to_string(),
                is_proton: false,
                name: "CC Contact".to_string(),
                group: MaybeEmptyString(None),
            }],
        },
        bcc_list: Default::default(),
        deleted: false,
        exclusive_location: None,
        expiration_time: 0.into(),
        external_id: None,
        flags: Default::default(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![],
        num_attachments: 0,
        display_order: 0,
        reply_tos: Default::default(),
        sender: MessageSender {
            address: "sender@void.org".to_owned(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: "Send InToVoid".to_string(),
        },
        size: 0,
        snooze_time: 0.into(),
        subject: "".to_string(),
        time: 0.into(),
        to_list: Default::default(),
        unread: false,
        custom_labels: vec![],
        row_id: None,
    }
}

fn existing_message_body_metadata() -> MessageBodyMetadata {
    MessageBodyMetadata {
        local_message_id: Some(local_msg_id()),
        remote_message_id: None,
        header: "".to_string(),
        mime_type: Default::default(),
        parsed_headers: Default::default(),
        attachments: vec![inline_attachment(), normal_attachment()],
        row_id: None,
    }
}

fn local_msg_id() -> LocalMessageId {
    424.into()
}

fn local_conversation_id() -> LocalConversationId {
    11111111.into()
}

fn remote_conversation_id() -> ConversationId {
    ConversationId::new("My remote conv id".to_owned())
}

fn local_address_id() -> LocalAddressId {
    9000.into()
}

fn remote_address_id() -> AddressId {
    AddressId::new("My remote addr id".to_owned())
}
const ADDRESS_SIGNATURE: &str = "My Address Signature";
const MAIL_SETTINGS_SIGNATURE: &str = "Mail settings signature";

fn inline_attachment_id() -> LocalAttachmentId {
    1245555.into()
}

fn normal_attachment_id() -> LocalAttachmentId {
    44623482634.into()
}

fn inline_attachment() -> Attachment {
    Attachment {
        local_id: Some(inline_attachment_id()),
        mime_type: attachment::MimeType::from_str("image/jpeg").unwrap(),
        filename: "image.jpeg".to_owned(),
        disposition: Disposition::Inline,
        ..Default::default()
    }
}
fn normal_attachment() -> Attachment {
    Attachment {
        local_id: Some(normal_attachment_id()),
        disposition: Disposition::Attachment,
        mime_type: attachment::MimeType::from_str("application/pdf").unwrap(),
        filename: "doc.pdf".to_owned(),
        ..Default::default()
    }
}
