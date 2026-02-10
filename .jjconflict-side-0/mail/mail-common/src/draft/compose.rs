use crate::datatypes::{MessageRecipient, MessageSender, ParsedHeaderValue};
use crate::draft::recipients::{ContactGroupResolver, MaybeEmptyString, RecipientList};
use crate::draft::{
    Error, ReplyMode, SaveError, SenderAddressChangeError, draft_v1, draft_v1::AttachmentRemovalId,
    draft_v1::DraftAttachmentRemovalQueuer,
};
use crate::models::{
    Attachment, CustomSettings, DraftAttachmentMetadata, MailSettings, Message,
    MessageBodyMetadata, MessageMimeType, MetadataId,
};
use crate::{MailContextError, MailContextResult, MailUserContext};
use anyhow::anyhow;
use chrono::DateTime;
use derive_more::Display;
use proton_canonical_email::CanonicalEmail;
use proton_core_api::services::proton::AddressId;
use proton_core_common::Platform;
use proton_core_common::datatypes::{AddressStatus, UnixTimestamp};
use proton_core_common::models::{Address, ModelIdExtension, PaidSubscription, User};
use proton_crypto_inbox::message::{
    DecryptableMessage, EncryptableDraft, EncryptedDraft, GettablePGPMessage, RawDecryptedBody,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::request_data::DraftRecipient;
use proton_mail_html_transformer::sanitizer::StripStyleSheets;
use proton_mail_html_transformer::transforms::ColorMode;
use proton_mail_html_transformer::transforms::styles::{
    BrowserCapabilities, IncludeFullStaticCss, InjectDarkModeOptions, dark_mode_for_plaintext,
};
use proton_mail_html_transformer::{Html2TextOptions, Transformer};
use stash::orm::Model as _;
use stash::stash::{StashError, Tether};
use std::fmt::Display;
use std::fmt::Write as _;
use tracing::error;

#[cfg(test)]
#[path = "../tests/draft/compose.rs"]
mod tests;

/// Copy all the data from the `source_message` into `message` taking
/// into account `reply_mode` of the draft.
pub(super) async fn patch_draft_with_reply_mode(
    contact_group_resolver: &impl ContactGroupResolver,
    draft: &mut draft_v1::Draft,
    source_message: &Message,
    source_message_body: &MessageBodyMetadata,
    reply_mode: ReplyMode,
) {
    let is_sent_message = source_message.is_sent();
    let canonical_sender_email = proton_canonical_email::canonicalize_auto(&draft.sender);
    let subject_prefix = match reply_mode {
        ReplyMode::Sender | ReplyMode::All => REPLY_PREFIX,
        ReplyMode::Forward => FORWARD_PREFIX,
    };

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
        }
        ReplyMode::All => {
            let (to, cc) = if is_sent_message {
                (
                    source_message.to_list.value.to_vec(),
                    source_message.cc_list.value.to_vec(),
                )
            } else {
                reply_all_recipients_for_received(
                    source_message,
                    source_message_body,
                    canonical_sender_email,
                )
            };
            draft.to_list =
                RecipientList::from_message_recipients(contact_group_resolver, to).await;
            draft.cc_list =
                RecipientList::from_message_recipients(contact_group_resolver, cc).await;
        }
        ReplyMode::Forward => {}
    }

    draft.subject = apply_prefix_to_subject(subject_prefix, &source_message.subject);
}

fn reply_all_recipients_for_received(
    source_message: &Message,
    source_message_body: &MessageBodyMetadata,
    canonical_sender_email: CanonicalEmail,
) -> (Vec<MessageRecipient>, Vec<MessageRecipient>) {
    let to = source_message_body
        .reply_tos
        .iter()
        .map(|recipient| MessageRecipient {
            address: recipient.address.clone(),
            is_proton: recipient.is_proton,
            name: recipient.name.clone(),
            group: MaybeEmptyString::from_option(None),
        })
        .collect();
    let cc = source_message
        .to_list
        .value
        .iter()
        .chain(&source_message.cc_list.value)
        .filter(|recipient| {
            proton_canonical_email::canonicalize_auto(&recipient.address) != canonical_sender_email
        })
        .cloned()
        .collect();

    (to, cc)
}

/// Build signature from mail settings.
///
/// `mime_type` is passed in explicitly since it can be overridden when reply to html content
/// for instance.
pub(super) fn get_full_signature(
    user: &User,
    address: &Address,
    mail_settings: &MailSettings,
    custom_settings: &CustomSettings,
    mime_type: MessageMimeType,
    platform: Platform,
) -> String {
    let line_break = match mime_type {
        MessageMimeType::TextHtml => HTML_LINE_BREAK,
        MessageMimeType::TextPlain => TEXT_LINE_BREAK,
    };

    let mut signature = String::new();

    let show_pm_signature = match platform {
        Platform::Desktop => !user.has_paid_mail_plan() || mail_settings.pm_signature.is_enabled(),
        Platform::Mobile => !user.has_paid_mail_plan(),
    };

    let show_mobile_signature = match platform {
        Platform::Desktop => false,
        Platform::Mobile => user.has_paid_mail_plan() && custom_settings.mobile_signature_enabled(),
    };

    _ = write!(
        signature,
        "{}",
        prepare_signature(&address.signature, mime_type)
    );

    if mime_type == MessageMimeType::TextHtml {
        // Wrap signature in a special `div` block so that we can replace
        // the signature if user changes the `from` address
        signature = format!("<div class=\"{PM_SIGNATURE_DIV_CLASS}\">{signature}</div>");
    }

    if show_pm_signature {
        if !signature.is_empty() {
            _ = write!(signature, "{line_break}{line_break}");
        }

        _ = write!(signature, "{}", prepare_signature(PM_SIGNATURE, mime_type));
    } else if show_mobile_signature {
        if !signature.is_empty() {
            _ = write!(signature, "{line_break}{line_break}");
        }

        _ = write!(
            signature,
            "{}",
            prepare_signature(custom_settings.mobile_signature(), mime_type)
        );
    }

    if signature.is_empty() {
        String::new()
    } else {
        format!("{line_break}{line_break}{line_break}{signature}")
    }
}

fn prepare_signature(signature: &str, mime_type: MessageMimeType) -> String {
    if mime_type == MessageMimeType::TextPlain {
        let sign = Transformer::new(signature)
            .to_plain_text(Html2TextOptions::default())
            .unwrap_or_else(|_| signature.to_owned());

        // html2text likes to insert "extra" whitelines, converting single-line
        // signatures like:
        //
        // ```
        // <b>cheers!</b>
        // ```
        //
        // ... into two-lined:
        //
        // ```
        // *cheers!*
        //
        // ```
        //
        // This is mildly awkward, because later it might happen that we'll have
        // to compose two signatures together (e.g. if you have both the address
        // and mobile signature activated), and that signature-composing
        // function already inserts its own newlines to separate the
        // "sub-signatures" it's given.
        //
        // So any extra "seemingly spurious" newlines we return here might cause
        // the final, composed signature to look extremely wide and weird:
        //
        // ```
        // address signature
        //
        //
        //
        // mobile signature
        // ```
        sign.trim_start_matches('\n')
            .trim_end_matches('\n')
            .to_owned()
    } else {
        signature.to_owned()
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

// Returns the encrypted body and the verification signatures
pub(super) async fn encrypt_draft_body(
    ctx: &MailUserContext,
    address_id: &AddressId,
    body: &str,
) -> Result<(EncryptedDraft, Vec<u8>), MailContextError> {
    let draft_body = DraftBody { body };
    let pgp = new_pgp_provider();

    let tether = ctx.user_stash().connection().await?;
    let unlocked_keys = ctx.unlocked_address_keys(&pgp, &tether, address_id).await?;

    let draft_encryption_key = unlocked_keys
        .primary_for_mail()
        .map_err(|_| {
            error!(
                "Unable to find the primary address key to encrypt the draft for address with id: {address_id}"
            );
            SaveError::AddressWithoutPrimaryKey(address_id.clone())
        })?;

    let encrypted = draft_body
        .encrypt_draft_body(&pgp, &draft_encryption_key)
        .map_err(|e| {
            error!("Failed to encrypt draft: {e:?}");
            MailContextError::Crypto
        })?;

    // To do proper signature verification if the user sends a message to themselves, we
    // need to store the draft signature with the draft.
    // Unfortunately, this means that we need to decrypt the message right after
    // encrypting.
    let encrypted_draft = EncryptedDraftMessage { body: &encrypted };

    let RawDecryptedBody::Plain { signatures, .. } =
        encrypted_draft.decrypt(&pgp, &unlocked_keys).map_err(|e| {
            error!("Failed to decrypt draft: {e:?}");
            MailContextError::Crypto
        })?
    else {
        error!("Saved draft message was not of plain type");
        return Err(MailContextError::Other(anyhow!("Unexpected draft state")));
    };

    Ok((encrypted, signatures))
}

struct EncryptedDraftMessage<'a> {
    body: &'a EncryptedDraft,
}

impl GettablePGPMessage for EncryptedDraftMessage<'_> {
    /// Return the encrypted body of the message, this is a PGP message which
    /// may then go on to be decrypted
    fn pgp_message(&self) -> &[u8] {
        self.body.as_bytes()
    }
}

impl DecryptableMessage for EncryptedDraftMessage<'_> {
    fn message_id(&self) -> Option<&str> {
        None
    }

    fn message_is_mime(&self) -> bool {
        false
    }
}

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

pub(super) fn prepare_text_reply(
    output: &mut String,
    message: &Message,
    original_body: &str,
    use_utc: bool,
) {
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
    output.push_str(original_body);
}

/// Converts htm to plain text. If an error occurs the original messages
/// is returned.
///
/// This method also performs basic html sanitizing before converting to text.
pub fn html_to_text(input: &str) -> String {
    let mut transformer = Transformer::new(input);

    transformer.transform_from_proton_schemes();
    transformer.add_noreferrer();
    transformer.strip_utm();
    transformer.strip_whitelist(StripStyleSheets::No);

    match transformer.to_plain_text(Html2TextOptions {
        decorate_links: false,
        decorate_images: false,
    }) {
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
    mime_type: MessageMimeType,
    body: &str,
    color_mode: ColorMode,
    capabilities: BrowserCapabilities,
    root_selector: String,
) -> DarkModeInjection {
    if mime_type == MessageMimeType::TextPlain {
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

pub fn sanitize_html_content(transformer: &mut Transformer, strip_style_sheets: StripStyleSheets) {
    transformer.transform_from_proton_schemes();
    transformer.add_noreferrer();
    transformer.strip_utm();
    transformer.strip_whitelist(strip_style_sheets);
    transformer.revert_dark_mode_in_inline_attributes();
}

pub fn sanitize_pasted_content(body: &str, mime_type: MessageMimeType) -> String {
    let mut transformer = match mime_type {
        MessageMimeType::TextHtml => Transformer::new(body),
        MessageMimeType::TextPlain => Transformer::new_text2html(body),
    };
    sanitize_html_content(&mut transformer, StripStyleSheets::Yes);
    transformer.extract_body()
}

pub fn maybe_sanitize(mime_type: MessageMimeType, body: &str) -> String {
    match mime_type {
        MessageMimeType::TextHtml => {
            let mut transformer = Transformer::new(body);
            sanitize_html_content(&mut transformer, StripStyleSheets::No);
            transformer.to_string()
        }

        MessageMimeType::TextPlain => body.to_owned(),
    }
}

/// Used only when creating a draft from existing message.
/// Extracts `<body>` innerHTML from the message.
fn sanitize_reply(body: &str) -> String {
    let mut html = Transformer::new(body);
    html.strip_whitelist(StripStyleSheets::No);
    html.extract_body()
}

/// Generates a reply similar to:
/// > On Tuesday, 01/01/2024 14:25, Slack <notification@slack.com> wrote:
fn generate_sender_reply(sender: &MessageSender, formatted_date: String, is_text: bool) -> String {
    if !sender.name.is_empty() && !sender.address.is_empty() {
        if is_text {
            format!(
                "{formatted_date} {} <{}> wrote:",
                sender.name.as_clear_text_str(),
                sender.address.as_clear_text_str()
            )
        } else {
            format!(
                "{formatted_date} {} &lt;{}&gt; wrote:",
                sender.name.as_clear_text_str(),
                sender.address.as_clear_text_str()
            )
        }
    } else if !sender.name.is_empty() {
        format!(
            "{formatted_date} {} wrote:",
            sender.name.as_clear_text_str()
        )
    } else {
        format!(
            "{formatted_date} {} wrote:",
            sender.address.as_clear_text_str()
        )
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
pub const FORWARD_PREFIX: &str = "Fw: ";

pub const DEFAULT_SUBJECT: &str = "(No Subject)";
pub const ORIGINAL_MESSAGE_BLOCK: &str = "-------- Original Message --------";
pub const BEGIN_QUOTE: &str = "<div class=\"protonmail_quote\">";
pub const BEGIN_BLOCKQUOTE: &str = "<blockquote class=\"protonmail_quote\">";
pub const CLOSE_QUOTE: &str = "</div>";
pub const CLOSE_BLOCKQUOTE: &str = "</blockquote>";
pub const HTML_LINE_BREAK: &str = "<br>";
pub const TEXT_LINE_BREAK: &str = "\n";

#[cfg(target_os = "android")]
pub const PM_SIGNATURE: &str = r#"Sent from <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> for Android."#;
#[cfg(target_os = "ios")]
pub const PM_SIGNATURE: &str =
    r#"Sent from <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> for iOS."#;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const PM_SIGNATURE: &str =
    r#"Sent from <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a>."#;

// this is the value the web client is using.
pub(super) const PM_SIGNATURE_DIV_CLASS: &str = "protonmail_signature_block-user";

pub fn apply_prefix_to_subject(prefix: &str, subject: &str) -> String {
    let trimmed_subject = subject.trim();
    if trimmed_subject.starts_with(prefix) {
        trimmed_subject.to_string()
    } else {
        format!("{prefix}{trimmed_subject}")
    }
}

pub struct DraftAddressChangeRequest {
    current_sender_email: String,
    current_address_id: AddressId,
    metadata_id: MetadataId,
    mime_type: MessageMimeType,
}

// Note: this type is currently separate from the draft implementation so that it can be executed
// in locations where the draft type is not safely shared (e.g.: TUI). A refactor is planned
// to make this work seamlessly.
pub enum DraftAddressChangeOutput {
    SenderOnly(String),
    Full(DraftAddressChangeFullAddressParams),
}

pub struct DraftAddressChangeFullAddressParams {
    pub old_signature: String,
    pub new_signature: String,
    pub address_id: AddressId,
    pub sender: String,
    pub is_byoe: bool,
}

impl DraftAddressChangeRequest {
    pub(super) fn new(
        metadata_id: MetadataId,
        sender_email: String,
        current_address_id: AddressId,
        mime_type: MessageMimeType,
    ) -> Self {
        Self {
            current_sender_email: sender_email,
            current_address_id,
            metadata_id,
            mime_type,
        }
    }

    #[tracing::instrument(skip(self, context, tether))]
    pub async fn apply(
        self,
        context: &MailUserContext,
        sender_email: String,
        address_id: AddressId,
        tether: &mut Tether,
    ) -> MailContextResult<Option<DraftAddressChangeOutput>> {
        tracing::info!("Updating sender address");

        if address_id == self.current_address_id {
            if self.current_sender_email != sender_email {
                return Ok(Some(DraftAddressChangeOutput::SenderOnly(sender_email)));
            }

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
                .queue(context.action_queue(), tether, context.origin())
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
        let new_signature = prepare_signature(&address.signature, self.mime_type);
        let old_signature = prepare_signature(&old_address.signature, self.mime_type);
        let is_byoe = address.is_byoe();

        Ok(Some(DraftAddressChangeOutput::Full(
            DraftAddressChangeFullAddressParams {
                old_signature,
                new_signature,
                address_id: address.remote_id.expect("Should be valid"),
                sender: sender_email,
                is_byoe,
            },
        )))
    }
}

/// Check for the presence of an alias in the original message and correctly patch
/// the address to have said alias.
pub fn resolve_sender_alias(
    address_email: &str,
    source_body_metadata: &MessageBodyMetadata,
) -> String {
    // We need to check if this header is present to correctly determine the destination
    // address of this email. This contains the email alias (email+alias@domain) which we
    // need to use to reply.
    source_body_metadata
        .parsed_header_value("X-Original-To")
        .map(|v| match v {
            ParsedHeaderValue::String(v) => v,
            ParsedHeaderValue::Array(mut a) => {
                tracing::warn!("Found array value for `X-Original-To`, using first value");
                a.drain(..).next().unwrap_or(address_email.to_owned())
            }
        })
        .map_or(address_email.to_owned(), |original_to| {
            // Check if the email has an alias attribute. We can't use the values as is, since
            // the backend does not resolve the emails in the sender correctly unless they
            // have the same capitalization. We have to extract the alias and apply it to
            // our identity.
            get_alias_component(&original_to).map_or(address_email.to_owned(), |alias| {
                address_email
                    .split_once('@')
                    .map_or(address_email.to_owned(), |(local, domain)| {
                        format!("{local}+{alias}@{domain}")
                    })
            })
        })
}

pub fn get_alias_component(email: &str) -> Option<&str> {
    if let (Some(plus_index), Some(at_index)) = (email.find("+"), email.find("@"))
        && at_index > plus_index
        && email.is_char_boundary(plus_index)
        && email.is_char_boundary(at_index)
    {
        Some(&email[plus_index + 1..at_index])
    } else {
        None
    }
}

pub(super) async fn draft_sender_addresses(
    sender_alias: Option<&String>,
    address_id: &AddressId,
    tether: &Tether,
) -> Result<Vec<Address>, StashError> {
    let addresses = Address::all_send_enabled(tether).await?;
    Ok(draft_sender_addresses_impl(
        sender_alias,
        address_id,
        addresses,
    ))
}

fn draft_sender_addresses_impl(
    sender_alias: Option<&String>,
    address_id: &AddressId,
    mut addresses: Vec<Address>,
) -> Vec<Address> {
    if let Some(sender_alias) = sender_alias {
        // Only add the alias to the list if the address id is still in the correct location
        if let Some(mut a) = addresses
            .iter()
            .find(|a| a.remote_id.as_ref() == Some(address_id))
            .cloned()
        {
            a.email = sender_alias.clone();
            // Alias always appear at the top.
            addresses.insert(0, a);
        }
    }
    addresses
}

#[derive(Debug, Display, Clone, Copy, Eq, PartialEq)]
pub enum DraftAddressValidationError {
    #[display("Replying/Sending from @pm.me addresses requires a paid subscription")]
    SubscriptionRequired,
    #[display("Address is disabled")]
    Disabled,
    #[display("Can not send from address")]
    CanNotSend,
    #[display("Can not receive on address")]
    CanNotReceive,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DraftAddressValidationResult {
    pub email: String,
    pub error: DraftAddressValidationError,
}

impl DraftAddressValidationResult {
    pub fn new(email: String, error: DraftAddressValidationError) -> Self {
        Self { email, error }
    }
}

pub fn validate_sender_address(
    address: &Address,
    user: &User,
) -> Option<DraftAddressValidationResult> {
    if address.status != AddressStatus::Enabled {
        return Some(DraftAddressValidationResult::new(
            address.email.clone(),
            DraftAddressValidationError::Disabled,
        ));
    }

    if !address.send {
        return Some(DraftAddressValidationResult::new(
            address.email.clone(),
            DraftAddressValidationError::CanNotSend,
        ));
    }

    if !address.receive {
        return Some(DraftAddressValidationResult::new(
            address.email.clone(),
            DraftAddressValidationError::CanNotReceive,
        ));
    }

    if address.email.to_lowercase().ends_with("@pm.me")
        && !user.subscribed.contains(PaidSubscription::MAIL)
    {
        return Some(DraftAddressValidationResult::new(
            address.email.clone(),
            DraftAddressValidationError::SubscriptionRequired,
        ));
    }

    None
}
