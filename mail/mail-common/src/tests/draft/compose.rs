pub use super::*;
use crate::datatypes::LocalConversationId;
use crate::datatypes::LocalMessageId;
use crate::datatypes::MessageFlags;
use crate::datatypes::MimeType;
use crate::datatypes::SystemLabelId;
use crate::datatypes::attachment;
use crate::datatypes::{Disposition, MessageRecipient, MessageRecipients, MessageSender};
use crate::datatypes::{LocalAttachmentId, ParsedHeaders};
use crate::decrypted_message::DecryptedMessageBody;
use crate::draft::MetadataId;
use crate::draft::Recipient;
use crate::draft::draft_v1::Draft;
use crate::draft::recipients::{MaybeEmptyString, NullContactGroupResolver};
use crate::models::{Attachment, MessageBodyMetadata, MessageReplyTo};
use insta::assert_snapshot;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::AddressFlags;
use proton_core_common::datatypes::{AddressStatus, AddressType, LocalAddressId};
use proton_core_common::datatypes::{UserMnemonicStatus, UserType};
use proton_core_common::models::{PaidSubscription, User};
use proton_mail_api::services::proton::prelude::ConversationId;
use std::str::FromStr;
use test_case::test_case;

#[test]
fn new_draft_message_creation() {
    let address = address();
    let mail_settings = mail_settings();
    let custom_settings = custom_settings();
    let draft = Draft::new_empty_draft(MetadataId(0), &address, &mail_settings, &custom_settings);

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
    assert!(
        draft
            .to_list
            .contains_email(source_message.sender.address.as_ref())
    );
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
    assert!(
        draft
            .to_list
            .contains_email(source_message.sender.address.as_ref())
    );
    assert!(
        draft.cc_list.contains_emails(
            source_message
                .cc_list
                .value
                .iter()
                .map(|v| v.address.as_ref())
        )
    );
    assert!(draft.bcc_list.is_empty());
    assert_eq!(attachments, vec![inline_attachment()])
}

#[tokio::test]
async fn reply_all_sent_by_me_takes_original_recipients() {
    let source_body_metadata = existing_message_body_metadata();
    let mut message = existing_message();
    message.label_ids.push(LabelId::all_sent());
    message.flags |= MessageFlags::SENT;

    let (draft, _, _) = create_reply_with_mime_and_body_and_message(
        ReplyMode::All,
        MimeType::TextPlain,
        "VIP invitation".to_owned(),
        source_body_metadata,
        MessageMimeType::TextPlain,
        message.clone(),
    )
    .await;

    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &message.subject)
    );

    assert_eq!(
        draft.to_list.as_strings(),
        vec![
            "to_contact_1@pm.me",
            "to_contact_2@pm.me",
            "address_email@proton.me",
            "to_and_cc_contact_4@pm.me",
        ],
        "to_list should contain all to-recipients"
    );
    assert_eq!(
        draft.cc_list.as_strings(),
        vec![
            "cc_contact_3@pm.me",
            "address_email@proton.me",
            "to_and_cc_contact_4@pm.me",
        ],
        "cc_list should contain cc-recipients without sender"
    );
    assert!(
        draft.bcc_list.is_empty(),
        "bcc_list should be empty for reply-all"
    );
}

#[tokio::test]
async fn reply_all_not_sent_by_me_updates_to_and_cc_recipients() {
    let source_body_metadata = existing_message_body_metadata();
    let message = existing_message();

    let reply_to = source_body_metadata.reply_tos.first().unwrap().clone();
    let (draft, _mime, _body) = create_reply_with_mime_and_body_and_message(
        ReplyMode::All,
        MimeType::TextPlain,
        "VIP invitation to release party".to_owned(),
        source_body_metadata,
        MessageMimeType::TextPlain,
        message.clone(),
    )
    .await;

    assert_eq!(
        draft.subject,
        apply_prefix_to_subject(REPLY_PREFIX, &message.subject)
    );

    assert_eq!(
        draft.to_list.as_strings(),
        vec![reply_to.address.into_clear_text_string()],
        "to_list should contain only sender"
    );
    assert_eq!(
        draft.cc_list.as_strings(),
        vec![
            "to_contact_1@pm.me",
            "to_contact_2@pm.me",
            "to_and_cc_contact_4@pm.me",
            "cc_contact_3@pm.me"
        ],
        "cc_list should contain to-recipients and cc-recipients without sender and duplicates"
    );
    assert!(
        draft.bcc_list.is_empty(),
        "bcc_list should be empty for reply-all"
    );
}

#[tokio::test]
async fn check_reply_signature_html() {
    let (draft, _, _) = create_reply_with(
        ReplyMode::All,
        MimeType::TextHtml,
        MessageMimeType::TextHtml,
    )
    .await;

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
        source_body,
        source_body_metadata,
        MessageMimeType::TextPlain,
    )
    .await;

    assert_snapshot!(draft.body());
}

#[tokio::test]
async fn check_reply_from_html_with_text_as_default() {
    let (draft, _, _) = create_reply_with(
        ReplyMode::All,
        MimeType::TextPlain,
        MessageMimeType::TextHtml,
    )
    .await;

    assert_snapshot!(draft.body());
    assert_eq!(draft.mime_type(), MessageMimeType::TextHtml);
}

#[tokio::test]
async fn check_reply_simple_login_alias() {
    // The reply data of the message body metadata will be different than the sender
    let mut source_body_metadata = existing_message_body_metadata();
    source_body_metadata.mime_type = MimeType::TextPlain;
    let expected_email = "hiddin_from_view@simplelogin.net".to_owned();
    source_body_metadata.reply_to.address = expected_email.clone().into();
    let source_body = "Hello World".to_owned();
    let expected_email = source_body_metadata.reply_to.address.clone();

    let (draft, _, _) = create_reply_with_mime_and_body(
        ReplyMode::Sender,
        MimeType::TextPlain,
        source_body,
        source_body_metadata,
        MessageMimeType::TextPlain,
    )
    .await;

    assert!(draft.to_list.contains_email(expected_email.as_ref()));
    assert!(
        !draft
            .to_list
            .contains_email(existing_message().sender.address.as_ref())
    );
}

#[tokio::test]
async fn check_reply_all_simple_login_alias() {
    // The reply data of the message body metadata will be different than the sender
    let mut source_body_metadata = existing_message_body_metadata();
    source_body_metadata.mime_type = MimeType::TextPlain;
    let expected_email = "hiddin_from_view@simplelogin.net".to_owned();
    source_body_metadata.reply_tos[0].address = expected_email.clone().into();
    let source_body = "Hello World".to_owned();
    let expected_email = source_body_metadata.reply_tos[0].address.clone();

    let (draft, _, _) = create_reply_with_mime_and_body(
        ReplyMode::All,
        MimeType::TextPlain,
        source_body,
        source_body_metadata,
        MessageMimeType::TextPlain,
    )
    .await;

    assert!(draft.to_list.contains_email(expected_email.as_ref()));
    assert!(
        !draft
            .to_list
            .contains_email(existing_message().sender.address.as_ref())
    );
}

#[tokio::test]
async fn reply_to_email_alias() {
    let mut source_body_metadata = existing_message_body_metadata();

    source_body_metadata
        .parsed_headers
        .headers
        .insert("X-Original-To".to_owned(), TEST_EMAIL_ALIAS.into());

    let source_body = "Hello World".to_owned();
    let mut source_message = existing_message();

    source_message.to_list.push(MessageRecipient {
        address: TEST_EMAIL_ALIAS.to_owned().into(),
        is_proton: false,
        name: TEST_EMAIL_DISPLAY_NAME.to_owned().into(),
        group: Default::default(),
    });

    let (draft, _, _) = create_reply_with_mime_and_body_and_message(
        ReplyMode::Sender,
        MimeType::TextPlain,
        source_body,
        source_body_metadata,
        MessageMimeType::TextPlain,
        source_message,
    )
    .await;

    assert!(!draft.to_list.contains_email(TEST_EMAIL_ALIAS));
    assert_eq!(draft.sender, TEST_EMAIL_ALIAS);
}

#[tokio::test]
async fn reply_strips_duplicate_sender_emails_and_aliases() {
    let mut source_body_metadata = existing_message_body_metadata();

    source_body_metadata
        .parsed_headers
        .headers
        .insert("X-Original-To".to_owned(), TEST_EMAIL_ALIAS.into());

    let source_body = "Hello World".to_owned();
    let mut source_message = existing_message();

    source_message.to_list.push(MessageRecipient {
        address: TEST_EMAIL_ALIAS.to_owned().into(),
        is_proton: false,
        name: TEST_EMAIL_DISPLAY_NAME.to_owned().into(),
        group: Default::default(),
    });

    source_message.cc_list.push(MessageRecipient {
        address: TEST_EMAIL_ALIAS_ALT.to_owned().into(),
        is_proton: false,
        name: TEST_EMAIL_DISPLAY_NAME.to_owned().into(),
        group: Default::default(),
    });

    let (draft, _, _) = create_reply_with_mime_and_body_and_message(
        ReplyMode::All,
        MimeType::TextPlain,
        source_body,
        source_body_metadata,
        MessageMimeType::TextPlain,
        source_message,
    )
    .await;

    assert!(!draft.to_list.contains_email(TEST_EMAIL_ALIAS));
    assert!(!draft.to_list.contains_email(TEST_EMAIL_ALIAS_ALT));
    assert!(!draft.cc_list.contains_email(TEST_EMAIL_ALIAS));
    assert!(!draft.cc_list.contains_email(TEST_EMAIL_ALIAS_ALT));
    assert_eq!(draft.sender, TEST_EMAIL_ALIAS);
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

mod signatures {
    use super::*;
    use crate::datatypes::PmSignature;
    use test_case::test_case;

    struct TestCase {
        given_address: fn() -> Address,
        given_mail_settings: fn() -> MailSettings,
        given_custom_settings: fn() -> CustomSettings,
        given_mime_type: MessageMimeType,
        expected_desktop: &'static str,
        expected_mobile: &'static str,
    }

    const TEST_NO_SIGNATURES: TestCase = TestCase {
        given_address: address,
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(false)
                .with_mobile_signature_enabled(false)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "",
        expected_mobile: "",
    };

    // On a fresh setup, we want to have the PM signature enabled by default
    const TEST_DEFAULT_CUSTOM_SETTINGS: TestCase = TestCase {
        given_address: address,
        given_mail_settings: mail_settings,
        given_custom_settings: custom_settings,
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "",
        expected_mobile: "\n\nSent from Proton Mail.",
    };

    const TEST_ADDRESS_SIGNATURE: TestCase = TestCase {
        given_address: || address().with_signature("cheers, jerry"),
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(true)
                .with_mobile_signature_enabled(false)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "\n\ncheers, jerry",
        expected_mobile: "\n\ncheers, jerry",
    };

    const TEST_MOBILE_SIGNATURE: TestCase = TestCase {
        given_address: address,
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_mobile_signature("sent from my iandroid")
                .with_mobile_signature_enabled(true)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "",
        expected_mobile: "\n\nsent from my iandroid",
    };

    const TEST_DISABLED_MOBILE_SIGNATURE: TestCase = TestCase {
        given_custom_settings: || {
            custom_settings()
                .with_mobile_signature("sent from my iandroid")
                .with_mobile_signature_enabled(false)
        },
        expected_desktop: "",
        expected_mobile: "",
        ..TEST_MOBILE_SIGNATURE
    };

    const TEST_ADDRESS_AND_MOBILE_SIGNATURE: TestCase = TestCase {
        given_address: || address().with_signature("cheers, jerry"),
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(true)
                .with_mobile_signature("sent from my iandroid")
                .with_mobile_signature_enabled(true)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "\n\ncheers, jerry",
        expected_mobile: "\n\ncheers, jerry\n\nsent from my iandroid",
    };

    const TEST_ADDRESS_AND_MOBILE_SIGNATURE_FREE: TestCase = TestCase {
        given_mail_settings: || mail_settings().with_pm_signature(PmSignature::LOCKED),
        expected_desktop: "\n\ncheers, jerry\n\nSent from Proton Mail.",
        expected_mobile: "\n\ncheers, jerry\n\nSent from Proton Mail.",
        ..TEST_ADDRESS_AND_MOBILE_SIGNATURE
    };

    const TEST_HTML_SIGNATURES: TestCase = TestCase {
        given_address: || address().with_signature("cheers, <b>jerry</b>"),
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(true)
                .with_mobile_signature("sent from <i>my</i> iandroid")
                .with_mobile_signature_enabled(true)
        },
        given_mime_type: MessageMimeType::TextHtml,
        expected_desktop: "<br><br><div class=\"protonmail_signature_block-user\">cheers, <b>jerry</b></div>",
        expected_mobile: "<br><br><div class=\"protonmail_signature_block-user\">cheers, <b>jerry</b></div><br><br>sent from <i>my</i> iandroid",
    };

    const TEST_HTML_SIGNATURES_TO_TEXT: TestCase = TestCase {
        given_address: || address().with_signature("cheers, <b>jerry</b>"),
        given_mail_settings: mail_settings,
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(true)
                .with_mobile_signature("sent from <i>my</i> iandroid")
                .with_mobile_signature_enabled(true)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "\n\ncheers, jerry",
        expected_mobile: "\n\ncheers, jerry\n\nsent from my iandroid",
    };

    // `MailSettings.signature` is deprecated and should not be accessed
    const TEST_MAIL_SETTINGS_SIGNATURE: TestCase = TestCase {
        given_address: address,
        given_mail_settings: || mail_settings().with_signature("med vänliga hälsningar"),
        given_custom_settings: || {
            custom_settings()
                .with_address_signature_enabled(false)
                .with_mobile_signature_enabled(false)
        },
        given_mime_type: MessageMimeType::TextPlain,
        expected_desktop: "",
        expected_mobile: "",
    };

    #[test_case(TEST_NO_SIGNATURES)]
    #[test_case(TEST_DEFAULT_CUSTOM_SETTINGS)]
    #[test_case(TEST_ADDRESS_SIGNATURE)]
    #[test_case(TEST_MOBILE_SIGNATURE)]
    #[test_case(TEST_DISABLED_MOBILE_SIGNATURE)]
    #[test_case(TEST_ADDRESS_AND_MOBILE_SIGNATURE)]
    #[test_case(TEST_ADDRESS_AND_MOBILE_SIGNATURE_FREE)]
    #[test_case(TEST_HTML_SIGNATURES)]
    #[test_case(TEST_HTML_SIGNATURES_TO_TEXT)]
    #[test_case(TEST_MAIL_SETTINGS_SIGNATURE)]
    fn test(case: TestCase) {
        for platform in [Platform::Desktop, Platform::Mobile] {
            let actual = get_full_signature(
                &(case.given_address)(),
                &(case.given_mail_settings)(),
                &(case.given_custom_settings)(),
                case.given_mime_type,
                platform,
            );

            match platform {
                Platform::Desktop => {
                    assert_eq!(case.expected_desktop, actual);
                }
                Platform::Mobile => {
                    assert_eq!(case.expected_mobile, actual);
                }
            }
        }
    }
}

#[test]
fn html_signature_converted_to_plain_text() {
    let signature = r#"<div style="font-family: Arial, sans-serif; font-size: 14px; color: rgb(0, 0, 0); background-color: rgb(255, 255, 255);">My Default Signature<br></div>"#;
    let address = address().with_signature(signature);
    let mail_settings = mail_settings();
    let custom_settings = custom_settings();
    let signature = get_full_signature(
        &address,
        &mail_settings,
        &custom_settings,
        MessageMimeType::TextPlain,
        Platform::Desktop,
    );

    assert_snapshot!(signature);
}

#[tokio::test]
async fn sanitize_draft_reply_html() {
    let (mut draft, _, _) = create_reply_with_mime_and_body(
        ReplyMode::All,
        MimeType::TextHtml,
        DRAFT_BODY_HTML.to_owned(),
        sanitize_message_body_metadata(MimeType::TextHtml),
        MessageMimeType::TextHtml,
    )
    .await;

    assert_snapshot!(draft.body());

    let sanitized = draft.body().to_owned();

    draft.sanitize_body();

    assert_snapshot!(draft.body());
    assert_eq!(sanitized, draft.body());
}

#[test_case(ReplyMode::Sender; "Sender")]
#[test_case(ReplyMode::All; "All")]
#[tokio::test]
async fn reply_to_sent_message_should_use_to_list_rather_than_sender(reply_mode: ReplyMode) {
    let source_body_metadata = existing_message_body_metadata();
    let mut message = existing_message();
    let sender_address = message.sender.address.clone();
    let to_address = "to_recipient@foo.com".to_owned();
    message.flags |= MessageFlags::SENT;
    message.label_ids.push(LabelId::sent());
    message.to_list.value.push(MessageRecipient {
        address: to_address.clone().into(),
        is_proton: false,
        name: "ToRecipient".into(),
        group: Default::default(),
    });
    let source_body = "Hello World".to_owned();

    let (draft, _, _) = create_reply_with_mime_and_body_and_message(
        reply_mode,
        MimeType::TextPlain,
        source_body,
        source_body_metadata,
        MessageMimeType::TextHtml,
        message,
    )
    .await;

    assert!(draft.to_list.contains_email(&to_address));
    assert!(!draft.to_list.contains_email(sender_address.as_ref()));
}

#[test]
fn resolve_sender_alias_mixed_case() {
    // It is possible to receive the alias address completely in lowercased, we need to make
    // sure it is applied to the original address instead.
    let email = "FooBar@proton.me";
    let body_metadata = alias_body_metadata("foobar+alias@proton.me".to_owned());
    let sender_email = resolve_sender_alias(email, &body_metadata);
    assert_eq!(sender_email, "FooBar+alias@proton.me");
}

#[test]
fn resolve_sender_alias_no_alias() {
    // if no alias exist, the value should be ignored
    let email = "FooBar@proton.me";
    let body_metadata = alias_body_metadata("omega@proton.me".to_owned());
    let sender_email = resolve_sender_alias(email, &body_metadata);
    assert_eq!(sender_email, "FooBar@proton.me");
}

#[test_case("foobar-alias@proton.me";"missing_plus")]
#[test_case("foobar+alias-proton.me";"missing_at")]
#[test_case("foobarproton.me";"missing_both")]
#[test_case("foobar@alias+proton.me";"swapped indices")]
fn resolve_sender_alias_invalid_value(alias: &str) {
    // if no alias exist, the value should be ignored
    let email = "FooBar@proton.me";
    let body_metadata = alias_body_metadata(alias.to_owned());
    let sender_email = resolve_sender_alias(email, &body_metadata);
    assert_eq!(sender_email, "FooBar@proton.me");
}

fn alias_body_metadata(alias: String) -> MessageBodyMetadata {
    let mut parsed_headers = ParsedHeaders::default();
    parsed_headers
        .headers
        .insert("X-Original-To".to_owned(), serde_json::Value::String(alias));
    MessageBodyMetadata {
        local_message_id: None,
        remote_message_id: None,
        header: "".to_string(),
        mime_type: Default::default(),
        parsed_headers,
        attachments: vec![],
        reply_to: Default::default(),
        reply_tos: vec![],
    }
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
    create_reply_with(reply_mode, MimeType::TextHtml, MessageMimeType::TextHtml).await
}

async fn create_reply_with(
    reply_mode: ReplyMode,
    draft_mime_type: MimeType,
    source_body_mime_type: MessageMimeType,
) -> (Draft, Message, Vec<Attachment>) {
    let source_body_metadata = existing_message_body_metadata();
    let source_body = "Hello World".to_owned();

    create_reply_with_mime_and_body(
        reply_mode,
        draft_mime_type,
        source_body,
        source_body_metadata,
        source_body_mime_type,
    )
    .await
}

async fn create_reply_with_mime_and_body(
    reply_mode: ReplyMode,
    draft_mime_type: MimeType,
    source_body: String,
    source_body_metadata: MessageBodyMetadata,
    source_body_mime_type: MessageMimeType,
) -> (Draft, Message, Vec<Attachment>) {
    let source_message = existing_message();

    create_reply_with_mime_and_body_and_message(
        reply_mode,
        draft_mime_type,
        source_body,
        source_body_metadata,
        source_body_mime_type,
        source_message,
    )
    .await
}

async fn create_reply_with_mime_and_body_and_message(
    reply_mode: ReplyMode,
    draft_mime_type: MimeType,
    source_body: String,
    source_body_metadata: MessageBodyMetadata,
    source_body_mime_type: MessageMimeType,
    source_message: Message,
) -> (Draft, Message, Vec<Attachment>) {
    let address = address();

    let mail_settings = MailSettings {
        draft_mime_type,
        ..MailSettings::default()
    };

    let custom_settings = custom_settings();

    let source_body = DecryptedMessageBody {
        body: source_body,
        metadata: source_body_metadata,
        mime_type: source_body_mime_type,
        pgp_subject: None,
        address_id: address.remote_id.clone().unwrap(),
        in_flight: Default::default(),
        decryption_error: None,
    };

    let resolver = NullContactGroupResolver {};

    let (draft, attachments) = Draft::new_draft_reply(
        &resolver,
        MetadataId(0),
        reply_mode,
        &address,
        &mail_settings,
        &custom_settings,
        &source_message,
        source_body,
        true,
        None,
    )
    .await;

    (draft, source_message, attachments)
}

fn address() -> Address {
    Address {
        local_id: Some(local_address_id()),
        remote_id: Some(remote_address_id()),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: TEST_EMAIL_DISPLAY_NAME.to_owned(),
        display_order: 0,
        domain_id: None,
        email: TEST_EMAIL.to_owned(),
        keys: Default::default(),
        proton_mx: false,
        receive: false,
        send: false,
        signature: String::new(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        flags: Some(AddressFlags::default()),
    }
}

const TEST_EMAIL: &str = "address_email@proton.me";
const TEST_EMAIL_DISPLAY_NAME: &str = "Addr Display Name";
const TEST_EMAIL_ALIAS: &str = "address_email+alias@proton.me";
const TEST_EMAIL_ALIAS_ALT: &str = "address_email+alias_alt@proton.me";

fn mail_settings() -> MailSettings {
    MailSettings::default()
}

fn custom_settings() -> CustomSettings {
    CustomSettings::default()
}

fn existing_message() -> Message {
    let sender_recipient = MessageRecipient {
        address: TEST_EMAIL.into(),
        is_proton: true,
        name: "".into(),
        group: MaybeEmptyString(None),
    };
    let duplicated_recipient = MessageRecipient {
        address: "to_and_cc_contact_4@pm.me".into(),
        is_proton: false,
        name: "TO AND CC Contact #4".into(),
        group: MaybeEmptyString(None),
    };
    Message {
        local_id: Some(local_msg_id()),
        remote_id: None,
        local_conversation_id: Some(local_conversation_id()),
        remote_conversation_id: Some(remote_conversation_id()),
        local_address_id: local_address_id(),
        remote_address_id: remote_address_id(),
        attachments_metadata: vec![],
        cc_list: MessageRecipients {
            value: vec![
                MessageRecipient {
                    address: "cc_contact_3@pm.me".into(),
                    is_proton: false,
                    name: "CC Contact #3".into(),
                    group: MaybeEmptyString(None),
                },
                sender_recipient.clone(),
                duplicated_recipient.clone(),
            ],
        },
        bcc_list: Default::default(),
        deleted: false,
        location: None,
        expiration_time: 0.into(),
        external_id: None,
        flags: Default::default(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![],
        num_attachments: 0,
        display_order: 0,
        sender: MessageSender {
            address: "sender@void.org".into(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: "Send InToVoid".into(),
        },
        size: 0,
        snooze_time: 0.into(),
        subject: "".to_string(),
        time: 0.into(),
        to_list: MessageRecipients {
            value: vec![
                MessageRecipient {
                    address: "to_contact_1@pm.me".into(),
                    is_proton: true,
                    name: "TO Contact #1".into(),
                    group: MaybeEmptyString(None),
                },
                MessageRecipient {
                    address: "to_contact_2@pm.me".into(),
                    is_proton: true,
                    name: "TO Contact #2".into(),
                    group: MaybeEmptyString(None),
                },
                sender_recipient.clone(),
                duplicated_recipient.clone(),
            ],
        },
        unread: false,
        custom_labels: vec![],
    }
}

fn existing_message_body_metadata() -> MessageBodyMetadata {
    let reply_to = MessageReplyTo {
        address: "sender@void.org".into(),
        bimi_selector: None,
        display_sender_image: false,
        is_proton: false,
        is_simple_login: false,
        name: "Send InToVoid".into(),
    };
    MessageBodyMetadata {
        local_message_id: Some(local_msg_id()),
        remote_message_id: None,
        header: "".to_string(),
        mime_type: Default::default(),
        parsed_headers: Default::default(),
        attachments: vec![inline_attachment(), normal_attachment()],
        reply_to: reply_to.clone(),
        reply_tos: vec![reply_to.clone()],
    }
}

#[test_case(ValidateAddressParams::disabled();"disable_address")]
#[test_case(ValidateAddressParams::deleting();"deleting_address")]
#[test_case(ValidateAddressParams::without_send(); "no_send")]
#[test_case(ValidateAddressParams::without_receive(); "no_receive")]
#[test_case(ValidateAddressParams::without_subscription(); "pm_without_subscription")]
#[test_case(ValidateAddressParams::with_subscription(); "pm_with_subscription")]
#[test_case(ValidateAddressParams::default(); "default")]
fn validate_address(params: ValidateAddressParams) {
    let address = Address {
        local_id: None,
        remote_id: None,
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "".to_string(),
        display_order: 0,
        domain_id: None,
        email: params.email,
        keys: Default::default(),
        proton_mx: false,
        receive: params.receive,
        send: params.send,
        signature: "".to_string(),
        signed_key_list: Default::default(),
        status: params.status,
        flags: Some(AddressFlags::default()),
    };

    let user = User {
        remote_id: None,
        create_time: Default::default(),
        credit: 0,
        currency: "".to_string(),
        delinquent: Default::default(),
        display_name: None,
        email: String::new(),
        keys: Default::default(),
        flags: Default::default(),
        max_space: 0,
        max_upload: 0,
        mnemonic_status: UserMnemonicStatus::Disabled,
        private: false,
        name: None,
        product_used_space: Default::default(),
        role: Default::default(),
        services: 0,
        subscribed: params.plans,
        to_migrate: false,
        used_space: 0,
        user_type: UserType::Proton,
    };

    let result = validate_sender_address(&address, &user);
    assert_eq!(result, params.expected);
}

struct ValidateAddressParams {
    email: String,
    status: AddressStatus,
    send: bool,
    receive: bool,
    plans: PaidSubscription,
    expected: Option<DraftAddressValidationResult>,
}

impl ValidateAddressParams {
    fn disabled() -> Self {
        Self {
            email: "foo@proton.ch".into(),
            status: AddressStatus::Disabled,
            send: true,
            receive: true,
            plans: PaidSubscription::empty(),
            expected: Some(DraftAddressValidationResult::new(
                "foo@proton.ch".into(),
                DraftAddressValidationError::Disabled,
            )),
        }
    }
    fn deleting() -> Self {
        Self {
            email: "foo@proton.ch".into(),
            status: AddressStatus::Deleting,
            send: true,
            receive: true,
            plans: PaidSubscription::empty(),
            expected: Some(DraftAddressValidationResult::new(
                "foo@proton.ch".into(),
                DraftAddressValidationError::Disabled,
            )),
        }
    }

    fn without_send() -> Self {
        Self {
            email: "foo@proton.ch".into(),
            status: AddressStatus::Enabled,
            send: false,
            receive: true,
            plans: PaidSubscription::empty(),
            expected: Some(DraftAddressValidationResult::new(
                "foo@proton.ch".into(),
                DraftAddressValidationError::CanNotSend,
            )),
        }
    }

    fn without_receive() -> Self {
        Self {
            email: "foo@proton.ch".into(),
            status: AddressStatus::Enabled,
            send: true,
            receive: false,
            plans: PaidSubscription::empty(),
            expected: Some(DraftAddressValidationResult::new(
                "foo@proton.ch".into(),
                DraftAddressValidationError::CanNotReceive,
            )),
        }
    }
    fn without_subscription() -> Self {
        Self {
            email: "foo@pm.me".into(),
            status: AddressStatus::Enabled,
            send: true,
            receive: true,
            plans: PaidSubscription::empty(),
            expected: Some(DraftAddressValidationResult::new(
                "foo@pm.me".into(),
                DraftAddressValidationError::SubscriptionRequired,
            )),
        }
    }
    fn with_subscription() -> Self {
        Self {
            email: "foo@pm.me".into(),
            status: AddressStatus::Enabled,
            send: true,
            receive: true,
            plans: PaidSubscription::MAIL,
            expected: None,
        }
    }

    fn default() -> Self {
        Self {
            email: "foo@proton.ch".into(),
            status: AddressStatus::Enabled,
            send: true,
            receive: true,
            plans: PaidSubscription::MAIL,
            expected: None,
        }
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

trait RecipientListTestExt {
    fn as_strings(&self) -> Vec<String>;
}

impl RecipientListTestExt for RecipientList {
    fn as_strings(&self) -> Vec<String> {
        self.recipients()
            .iter()
            .map(|r| match r {
                Recipient::Single(s) => s.email.clone().into_clear_text_string(),
                Recipient::Group(g) => g.group_name.clone().into_inner(),
            })
            .collect()
    }
}

#[test]
fn test_sanitize_pasted_content() {
    let html_with_styles = r##"<html>
            <head><style>.test {color: red;}</style></head>
            <body>
                <div style="margin:10px;" bgcolor="#fff">
                    <p data-proton-original-style="font-size:14px;">Pasted content</p>
                </div>
            </body>
        </html>"##;

    let result = crate::draft::compose::sanitize_pasted_content(html_with_styles);

    insta::assert_snapshot!(result);
}

#[test]
fn test_sanitize_html_content_with_styles_no() {
    let html = r#"<p style="color:red;" bgcolor="blue">Content</p>"#;
    let mut transformer = proton_mail_html_transformer::Transformer::new(html);

    crate::draft::compose::sanitize_html_content(
        &mut transformer,
        proton_mail_html_transformer::sanitizer::StripStyleSheets::No,
    );
    let result = transformer.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn test_sanitize_html_content_with_styles_yes() {
    let html = r#"<p style="color:red;" bgcolor="blue">Content</p>"#;
    let mut transformer = proton_mail_html_transformer::Transformer::new(html);

    crate::draft::compose::sanitize_html_content(
        &mut transformer,
        proton_mail_html_transformer::sanitizer::StripStyleSheets::Yes,
    );
    let result = transformer.to_string();

    insta::assert_snapshot!(result);
}
