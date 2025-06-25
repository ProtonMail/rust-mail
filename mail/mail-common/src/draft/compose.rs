use crate::datatypes::{MessageRecipient, MessageSender, MimeType};
use crate::draft::recipients::{ContactGroupResolver, MaybeEmptyString, RecipientList};
use crate::draft::{
    AttachmentRemovalId, Draft, DraftAttachmentRemovalQueuer, Error, ReplyMode, SaveError,
    SenderAddressChangeError,
};
use crate::models::{
    Attachment, DraftAttachmentMetadata, MailSettings, Message, MessageBodyMetadata, MetadataId,
};
use crate::{MailContextError, MailContextResult, MailUserContext};
use chrono::DateTime;
use proton_core_api::services::proton::AddressId;
use proton_core_common::datatypes::{AddressStatus, UnixTimestamp};
use proton_core_common::models::{Address, ModelIdExtension};
use proton_crypto_inbox::message::{EncryptableDraft, EncryptedDraft};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::request_data::DraftRecipient;
use proton_mail_html_transformer::transforms::ColorMode;
use proton_mail_html_transformer::transforms::styles::{
    BrowserCapabilities, IncludeFullStaticCss, InjectDarkModeOptions, dark_mode_for_plaintext,
};
use proton_mail_html_transformer::{Html2TextOptions, Transformer};
use stash::stash::Tether;
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
    source_message_body: &MessageBodyMetadata,
    reply_mode: ReplyMode,
) {
    let is_sent_message = source_message.is_sent();
    let canonical_sender_email = proton_canonical_email::canonicalize_auto(&draft.sender);

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
                draft.to_list = RecipientList::from_message_reply_to(std::iter::once(
                    source_message_body.reply_to.clone(),
                ));
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
                let reply_tos_iter =
                    source_message_body
                        .reply_tos
                        .iter()
                        .map(|v| MessageRecipient {
                            address: v.address.clone(),
                            is_proton: v.is_proton,
                            name: v.name.clone(),
                            group: MaybeEmptyString::from_option(None),
                        });
                let to_list_iter = source_message
                    .to_list
                    .value
                    .iter()
                    .filter(|v| {
                        proton_canonical_email::canonicalize_auto(&v.address)
                            != canonical_sender_email
                    })
                    .cloned();
                draft.to_list = RecipientList::from_message_recipients(
                    contact_group_resolver,
                    reply_tos_iter.chain(to_list_iter),
                )
                .await;
            }
            draft.cc_list = RecipientList::from_message_recipients(
                contact_group_resolver,
                source_message
                    .cc_list
                    .value
                    .iter()
                    .filter(|v| {
                        proton_canonical_email::canonicalize_auto(&v.address)
                            != canonical_sender_email
                    })
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
///
/// `mime_type` is passed in explicitly since it can be overridden when reply to html content
/// for instance.
pub(super) fn get_full_signature(
    address: &Address,
    mail_settings: &MailSettings,
    mime_type: MimeType,
) -> String {
    let line_break = if mime_type == MimeType::TextHtml {
        HTML_LINE_BREAK
    } else {
        "\n"
    };
    let mut signature = get_address_signature(address, mime_type);
    if mime_type == MimeType::TextHtml {
        // wrap the signature in a div block so we can replace it later
        signature = format!("<div class=\"{PM_SIGNATURE_DIV_CLASS}\">{signature}</div>");
    }

    if mail_settings.pm_signature.is_enabled() {
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

pub(super) fn get_address_signature(address: &Address, mime_type: MimeType) -> String {
    if mime_type == MimeType::TextPlain {
        // convert signature from html to text. All our signatures are generated
        // on and are stored as web snippets.
        Transformer::new(&address.signature)
            .to_plain_text(Html2TextOptions {
                link_foot_notes: false,
                ..Default::default()
            })
            .unwrap_or(address.signature.clone())
    } else {
        address.signature.clone()
    }
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
    let pgp = new_pgp_provider();

    let tether = ctx.user_stash().connection();
    let unlocked_keys = ctx.unlocked_address_keys(&pgp, &tether, address_id).await?;

    let draft_encryption_key = unlocked_keys
        .primary_for_mail()
        .map_err(|_| {
            error!(
                "Unable to find the primary address key to encrypt the draft for address with id: {address_id}"
            );
            SaveError::AddressWithoutPrimaryKey(address_id.clone())
        })?;

    draft_body
        .encrypt_draft_body(&pgp, &draft_encryption_key)
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
            head: dark_mode_for_plaintext(color_mode, capabilities, IncludeFullStaticCss::Yes)
                .to_owned(),
            body: body.to_owned(),
        };
    }

    let mut transformer = Transformer::new(body);
    let head = transformer.inject_dark_mode_to_another_target(InjectDarkModeOptions {
        // We do not trust the sender, so we will always inject dark mode.
        sender: None,
        mode: color_mode,
        capabilities,
        root_selector,
        include_full_static_css: IncludeFullStaticCss::Yes,
        // Therefore we do not populate trusted senders list.
        trusted_senders: &[],
    });
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
    html.strip_whitelist();
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
pub const HTML_LINE_BREAK: &str = "<br>";

const PM_SIGNATURE_HTML: &str = r#"Sent with <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> secure email."#;

const PM_SIGNATURE_PLAIN_TEXT: &str = "Sent with Proton Mail secure email.";

// this is the value the web client is using.
pub(super) const PM_SIGNATURE_DIV_CLASS: &str = "protonmail_signature_block-user";

fn apply_prefix_to_subject(prefix: &str, subject: &str) -> String {
    let trimmed_subject = subject.trim();
    if trimmed_subject.starts_with(prefix) {
        trimmed_subject.to_string()
    } else {
        format!("{prefix} {trimmed_subject}")
    }
}

pub struct DraftAddressChangeRequest {
    current_address_id: AddressId,
    metadata_id: MetadataId,
    mime_type: MimeType,
}

// Note: this type is currently separate from the draft implementation so that it can be executed
// in locations where the draft type is not safely shared (e.g.: TUI). A refactor is planned
// to make this work seamlessly.
pub struct DraftAddressChangeOutput {
    pub(super) old_signature: String,
    pub(super) new_signature: String,
    pub(super) address_id: AddressId,
    pub(super) sender: String,
}

impl DraftAddressChangeRequest {
    pub(super) fn new(
        metadata_id: MetadataId,
        current_address_id: AddressId,
        mime_type: MimeType,
    ) -> Self {
        Self {
            current_address_id,
            metadata_id,
            mime_type,
        }
    }

    #[tracing::instrument(level = "debug", skip(self, context, tether))]
    pub async fn apply(
        self,
        context: &MailUserContext,
        address_id: AddressId,
        tether: &mut Tether,
    ) -> MailContextResult<Option<DraftAddressChangeOutput>> {
        tracing::info!("Updating sender address");
        if address_id == self.current_address_id {
            // Nothing to do
            return Ok(None);
        }
        let address = Address::find_by_remote_id(address_id.clone(), tether)
            .await?
            .ok_or(SenderAddressChangeError::AddressNotFound(
                address_id.clone(),
            ))?;

        let old_address = Address::find_by_remote_id(self.current_address_id.clone(), tether)
            .await?
            .ok_or(SenderAddressChangeError::AddressNotFound(
                self.current_address_id.clone(),
            ))?;

        if address.status != AddressStatus::Enabled {
            return Err(
                Error::SenderAddressChange(SenderAddressChangeError::AddressDisabled(
                    address_id.clone(),
                ))
                .into(),
            );
        }

        if !address.send {
            return Err(Error::SenderAddressChange(
                SenderAddressChangeError::AddressNotSendEnabled(address_id.clone()),
            )
            .into());
        }

        let mail_settings = MailSettings::get_or_default(tether).await;
        if mail_settings.attach_public_key {
            let old_public_key_attachment =
                Attachment::gen_public_key(context, &old_address, tether).await?;
            let public_key_attachment =
                Attachment::gen_public_key(context, &address, tether).await?;
            let draft_attachments =
                DraftAttachmentMetadata::public_key_attachments(self.metadata_id, tether).await?;

            if let Some(attachment) = draft_attachments.iter().find(|attachment| {
                attachment.filename == old_public_key_attachment.attachment.filename
            }) {
                tracing::info!(
                    "Removing public key for old address ({})",
                    attachment.local_id.unwrap()
                );
                DraftAttachmentRemovalQueuer::new(
                    self.metadata_id,
                    AttachmentRemovalId::Local(attachment.local_id.unwrap()),
                )
                .queue(context.action_queue(), tether)
                .await
                .inspect_err(|e| error!("Failed to remove old public key attachment: {e:?}"))?;
            }

            if !draft_attachments.iter().any(|attachment| {
                attachment.is_public_key_attachment()
                    && attachment.filename == public_key_attachment.attachment.filename
            }) {
                tracing::info!("Public key for new address is not present, attaching");
                tether
                    .tx::<_, _, MailContextError>(async |tx| {
                        let attachment = public_key_attachment.store(context, tx).await?;
                        DraftAttachmentMetadata::pending(
                            self.metadata_id,
                            attachment.local_id.unwrap(),
                            0,
                            true,
                        )
                        .save(tx)
                        .await?;
                        Ok(())
                    })
                    .await?;
            }
        };

        // We only want to replace the address signature, not the whole setup with spacing
        // and the extra spacing and optional "Sent from ProtonMail".
        // This also more resilient to body changes after editing.
        let new_signature = get_address_signature(&address, self.mime_type);
        let old_signature = get_address_signature(&old_address, self.mime_type);

        Ok(Some(DraftAddressChangeOutput {
            old_signature,
            new_signature,
            address_id: address.remote_id.expect("Should be valid"),
            sender: address.email,
        }))
    }
}
