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
use proton_mail_html_transformer::Transformer;
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
    // Copy over the addresses based on reply mode
    match reply_mode {
        ReplyMode::Sender => {
            draft.to_list = RecipientList::from_message_recipients(
                contact_group_resolver,
                std::iter::once(source_message.sender.clone().into()),
            )
            .await;
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::All => {
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
            draft.subject = apply_prefix_to_subject(REPLY_PREFIX, &source_message.subject);
        }
        ReplyMode::Forward => {
            draft.subject = apply_prefix_to_subject(FORWARD_PREFIX, &source_message.subject);
        }
    }
}

/// Build signature from mail settings.
pub(super) fn get_signature(address: &Address, mail_settings: &MailSettings) -> String {
    let line_break = if mail_settings.draft_mime_type == MimeType::TextHtml {
        HTML_LINE_BREAK
    } else {
        "\n"
    };
    let mut signature = address.signature.clone();

    if mail_settings.pm_signature != PmSignature::Disabled {
        signature.push_str(line_break);
        signature.push_str(line_break);
        if mail_settings.draft_mime_type == MimeType::TextHtml {
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
    match transformer.to_plain_text() {
        Ok(text_body) => text_body,
        Err(e) => {
            error!("Failed to convert html to text: {e:?}");
            input.to_owned()
        }
    }
}

/// Only html content is sanitized, plain text is ignored.
pub fn maybe_sanitize(mime_type: MimeType, body: String) -> String {
    // There is no point in sanitizing content that is not HTML.
    if mime_type != MimeType::TextHtml {
        return body;
    }
    let mut transformer = Transformer::new(&body);
    transformer.add_noreferrer();
    transformer.strip_utm();
    transformer.strip_whitelist();

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
    // TODO(wpolak): In following MR's:
    // * Inject dark mode
    //     * Make sure dark mode is reversible
    html.move_styles_to_body();
    // * Sanitize `<style>` in `<body>` so that selectors are pointing to
    // `.protonmail_quote` (to prevent style bleeding)
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
