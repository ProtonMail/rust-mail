use crate::datatypes::{MessageRecipient, MessageSender, MimeType, PmSignature};
use crate::draft::recipients::{ContactGroupResolver, RecipientList};
use crate::draft::{Draft, ReplyMode, SaveError};
use crate::models::{MailSettings, Message};
use crate::{MailContextError, MailUserContext};
use chrono::DateTime;
use proton_core_api::services::proton::AddressId;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::Address;
use proton_crypto_inbox::message::{EncryptableDraft, EncryptedDraft};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::request_data::DraftRecipient;
use proton_mail_html_transformer::transforms::ColorMode;
use proton_mail_html_transformer::transforms::styles::{
    BrowserCapabilities, dark_mode_for_plaintext,
};
use proton_mail_html_transformer::{Html2TextOptions, Transformer};
use std::borrow::Cow;
use std::fmt::Display;
use tracing::error;

#[cfg(test)]
#[path = "../tests/draft/compose.rs"]
mod tests;

/// Copy all the data from the `source_message` into `message` taking
/// into account `reply_mode` of the draft.
pub(super) async fn patch_draft_with_reply_mode(
    contact_group_resolver: &impl ContactGroupResolver,
    draft: &mut Draft,
    source_message: &Message,
    reply_mode: ReplyMode,
    sender_address: &Address,
) {
    let is_sent_message = source_message.is_sent();

    // Copy over the addresses based on reply mode
    match reply_mode {
        ReplyMode::Sender => {
            if is_sent_message {
                draft.to_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    source_message.to_list.value.iter().cloned(),
                )
                .await;
            } else {
                draft.to_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    std::iter::once(source_message.sender.clone().into()),
                )
                .await;
            }
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::All => {
            if is_sent_message {
                draft.to_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    source_message.to_list.value.iter().cloned(),
                )
                .await;
            } else {
                draft.to_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    std::iter::once(source_message.sender.clone().into()).chain(
                        source_message
                            .to_list
                            .value
                            .iter()
                            .filter(|v| v.address != sender_address.email)
                            .cloned(),
                    ),
                )
                .await;
                draft.cc_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    source_message
                        .cc_list
                        .value
                        .iter()
                        .filter(|v| v.address != sender_address.email)
                        .cloned(),
                )
                .await;
            }
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::Forward => {
            draft.subject = apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject);
        }
    }
}

/// Build signature from mail settings.
///
/// `mime_type` is passed in explicitly since it can be overridden when reply to html content
/// for instance.
pub(super) fn get_signature(
    address: &Address,
    mail_settings: &MailSettings,
    mime_type: MimeType,
) -> String {
    let line_break = if mime_type == MimeType::TextHtml {
        HTML_LINE_BREAK
    } else {
        "\n"
    };
    let mut signature = if mime_type == MimeType::TextPlain {
        // convert signature from html to text, since it is possible there html content in it.
        Transformer::html2text_str(
            &address.signature,
            Html2TextOptions {
                link_foot_notes: false,
                ..Default::default()
            },
        )
        .unwrap_or(address.signature.clone())
    } else {
        address.signature.clone()
    };

    if mail_settings.pm_signature != PmSignature::Disabled {
        signature.push_str(line_break);
        signature.push_str(line_break);
        if mime_type == MimeType::TextHtml {
            signature.push_str(PM_SIGNATURE_HTML);
        } else {
            signature.push_str(PM_SIGNATURE_PLAIN_TEXT);
        }
    }

    if !signature.is_empty() {
        signature.insert_str(0, &format!("{line_break}{line_break}"));
    }

    signature
}

pub(crate) fn recipient_from_message_sender(
    recipients: &[MessageRecipient],
) -> Vec<DraftRecipient> {
    recipients
        .iter()
        .map(|v| DraftRecipient {
            address: v.address.clone(),
            name: v.name.clone(),
            group: v.group.clone().into_option(),
        })
        .collect()
}

struct DraftBody<'b> {
    body: &'b str,
}

impl EncryptableDraft for DraftBody<'_> {
    fn plaintext_message_body(&self) -> &[u8] {
        self.body.as_bytes()
    }
}

/// Encrypt the `body` with the key for `address_id`.
pub(super) async fn encrypt_draft_body(
    ctx: &MailUserContext,
    address_id: &AddressId,
    body: &str,
) -> Result<EncryptedDraft, MailContextError> {
    let draft_body = DraftBody { body };
    let pgp_provider = new_pgp_provider();

    let tether = ctx.user_stash().connection();
    let unlocked_keys = ctx
        .unlocked_address_keys(&pgp_provider, &tether, address_id)
        .await?;

    let draft_encryption_key = unlocked_keys
        .primary_for_mail()
        .map_err(|_| {
            error!(
                "Unable to find the primary address key to encrypt the draft for address with id: {address_id}"
            );
            SaveError::AddressWithoutPrimaryKey(address_id.clone())
        })?;
    draft_body
        .encrypt_draft_body(&pgp_provider, &draft_encryption_key)
        .map_err(|e| {
            error!("Failed to encrypt draft: {e:?}");
            MailContextError::Crypto
        })
}

/// Create a new timestamp.
pub(crate) fn create_timestamp() -> UnixTimestamp {
    UnixTimestamp::now()
}

/// Generate HTML reply body for a message.
pub(super) fn prepare_html_reply(
    output: &mut String,
    message: &Message,
    original_body: &str,
    use_utc: bool,
) {
    let sender_reply = generate_sender_reply(
        &message.sender,
        format_date_from_timestamp(message.time, use_utc),
        false,
    );
    output.reserve((ORIGINAL_MESSAGE_BLOCK.len() * 2) + original_body.len());
    output.push_str(BEGIN_QUOTE);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(ORIGINAL_MESSAGE_BLOCK);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(&sender_reply);
    output.push_str(HTML_LINE_BREAK);
    output.push_str(BEGIN_BLOCKQUOTE);
    output.push_str(&sanitize_reply(original_body));
    output.push_str(CLOSE_BLOCKQUOTE);
    output.push_str(CLOSE_QUOTE);
}

/// Generate a plain text reply body for a message.
pub(super) fn prepare_plain_text_reply(
    output: &mut String,
    message: &Message,
    original_body: &str,
    original_body_mime_type: MimeType,
    use_utc: bool,
) {
    let mut original_body = Cow::Borrowed(original_body);
    // Convert body to text if source is html
    if original_body_mime_type == MimeType::TextHtml {
        original_body = Cow::Owned(html_to_text(original_body.as_ref()));
    }

    let sender_reply = generate_sender_reply(
        &message.sender,
        format_date_from_timestamp(message.time, use_utc),
        true,
    );

    output.reserve((ORIGINAL_MESSAGE_BLOCK.len() * 2) + original_body.len());
    output.push('\n');
    output.push('\n');
    output.push_str(ORIGINAL_MESSAGE_BLOCK);
    output.push('\n');
    output.push_str(&sender_reply);
    output.push('\n');
    output.push_str(&original_body);
}

/// Converts htm to plain text. If an error occurs the original messages
/// is returned.
///
/// This method also performs basic html sanitizing before converting to text.
pub fn html_to_text(input: &str) -> String {
    let mut transformer = Transformer::new(input);
    transformer.add_noreferrer();
    transformer.strip_utm();
    transformer.strip_whitelist();
    match transformer.to_plain_text(Default::default()) {
        Ok(text_body) => text_body,
        Err(e) => {
            error!("Failed to convert html to text: {e:?}");
            input.to_owned()
        }
    }
}

pub struct DarkModeInjection {
    /// Composer head. Not sent to the recipient.
    pub head: String,
    /// New body of the draft. Sent to the recipient.
    /// Needs reverse operation before sending.
    pub body: String,
}

/// This function adds dark mode support to the composer. It does modify original body only in the context
/// of removing `!important` flag from styles and attributes.
///
/// Supplement CSS are not injected, instead the function returns the head in a separate string.
///
/// * `root_selector` - the CSS selector of the root of message.
///   In case of viewing message, it is usually data attribute pointing to the `html` tag.
///   In case of composer, it is ID pointing to custom editor that wraps the message.
///   Used to create a selector with bigger specificity than any provided by the sender.
pub fn inject_dark_mode(
    mime_type: MimeType,
    body: &str,
    color_mode: ColorMode,
    capabilities: BrowserCapabilities,
    root_selector: String,
) -> DarkModeInjection {
    if mime_type == MimeType::TextPlain {
        return DarkModeInjection {
            head: dark_mode_for_plaintext(color_mode, capabilities).to_owned(),
            body: body.to_owned(),
        };
    }

    let mut transformer = Transformer::new(body);
    // For now we set sender to None which means that we trust the sender.
    let head = transformer.inject_dark_mode_to_another_target(
        None,
        color_mode,
        capabilities,
        root_selector,
    );
    DarkModeInjection {
        head,
        body: transformer.to_string(),
    }
}

/// Only html content is sanitized, plain text is ignored.
pub fn maybe_sanitize(mime_type: MimeType, body: &str) -> String {
    // There is no point in sanitizing content that is not HTML.
    if mime_type != MimeType::TextHtml {
        return body.to_owned();
    }
    let mut transformer = Transformer::new(body);
    transformer.add_noreferrer();
    transformer.strip_utm();
    transformer.strip_whitelist();
    transformer.revert_dark_mode_in_inline_attributes();

    transformer.to_string()
}

/// Used only when creating a draft from existing message.
/// Extracts `<body>` innerHTML from the message.
///
/// # Parameters
///
/// * `body` - message body, containing full `<html>`
fn sanitize_reply(body: &str) -> String {
    let mut html = Transformer::new(body);
    html.move_styles_to_body();
    html.extract_body()
}

/// Generates a reply similar to:
/// > On Tuesday, 01/01/2024 14:25, Slack <notification@slack.com> wrote:
fn generate_sender_reply(sender: &MessageSender, formatted_date: String, is_text: bool) -> String {
    if !sender.name.is_empty() && !sender.address.is_empty() {
        if is_text {
            format!(
                "{formatted_date} {} <{}> wrote:",
                sender.name, sender.address
            )
        } else {
            format!(
                "{formatted_date} {} &lt;{}&gt; wrote:",
                sender.name, sender.address
            )
        }
    } else if !sender.name.is_empty() {
        format!("{formatted_date} {} wrote:", sender.name)
    } else {
        format!("{formatted_date} {} wrote:", sender.address)
    }
}

fn format_date_from_timestamp(timestamp: UnixTimestamp, use_utc: bool) -> String {
    if use_utc {
        format_date(timestamp.to_date_time_utc().unwrap_or_default())
    } else {
        format_date(timestamp.to_date_time().unwrap_or_default())
    }
}

fn format_date<Tz: chrono::TimeZone>(date: DateTime<Tz>) -> String
where
    <Tz as chrono::TimeZone>::Offset: Display,
{
    //On Tuesday, 01/01/2024 14:25
    // Localize date representation
    date.format("On %A, %x at %H:%M").to_string()
}

pub const REPLY_PREFIX: &str = "Re: ";
pub const FORWARD_PREFIX: &str = "Fwd: ";

pub const DEFAULT_SUBJECT: &str = "(No Subject)";
pub const ORIGINAL_MESSAGE_BLOCK: &str = "-------- Original Message --------";
pub const BEGIN_QUOTE: &str = "<div class=\"protonmail_quote\">";
pub const BEGIN_BLOCKQUOTE: &str = "<blockquote class=\"protonmail_quote\">";
pub const CLOSE_QUOTE: &str = "</div>";
pub const CLOSE_BLOCKQUOTE: &str = "</blockquote>";
pub const HTML_LINE_BREAK: &str = "<br/>";

const PM_SIGNATURE_HTML: &str = r#"Sent with <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> secure email."#;

const PM_SIGNATURE_PLAIN_TEXT: &str = "Sent with Proton Mail secure email.";

fn apply_prefix_to_subject(prefix: &str, subject: &str) -> String {
    let trimmed_subject = subject.trim();
    if trimmed_subject.starts_with(prefix) {
        trimmed_subject.to_string()
    } else {
        format!("{prefix} {trimmed_subject}")
    }
}
