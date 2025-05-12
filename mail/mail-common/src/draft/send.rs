use crate::datatypes::{Disposition, MimeType};
use crate::draft::recipients::ValidationState;
use crate::draft::{PackageError, SaveOrSendError, compose::html_to_text};
use crate::models::Attachment;
use crate::{MailContextError, MailContextResult, MailUserContext};
use proton_action_queue::action::WriterGuard;
use proton_crypto_account::keys::{
    PrimaryUnlockedAddressKey, UnlockedAddressKey, UnlockedAddressKeys,
};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::attachment::DecryptableAttachment;
use proton_crypto_inbox::keys::{
    ComposerPreference, CryptoMailSettings, InboxSessionKey, PackageCryptoType, SendPreferences,
};
use proton_crypto_inbox::message::packages::{
    EncryptedPackageBody, PackageMimeType, package_body_encrypt,
};
use proton_crypto_inbox::proton_crypto_inbox_mime::write::InboxMimeBuilder;
use proton_mail_api::services::proton::request_data::{
    AddressSubPackage, Package, PackageSignaturesMode,
};
use stash::stash::RunTransaction;
use std::collections::{HashMap, HashSet};
use tracing::{Instrument, debug, debug_span, error};

/// Loads the send preferences for each recipient of the message.
pub async fn load_send_preferences_for_recipients<Provider: PGPProviderSync>(
    context: &MailUserContext,
    pgp_provider: &Provider,
    rt: &mut impl RunTransaction,
    recipient_emails: &[String],
    crypto_mail_settings: CryptoMailSettings,
) -> MailContextResult<HashMap<String, SendPreferences<Provider::PublicKey>>> {
    let mut send_preferences = HashMap::with_capacity(recipient_emails.len());
    for recipient in recipient_emails {
        let send_preference = context
            .recipient_send_preferences(
                pgp_provider,
                rt,
                recipient,
                crypto_mail_settings,
                ComposerPreference::default(),
            )
            .await
            .map_err(|err| {
                error!(
                    "Failed to load send preferences for recipient {}: {}",
                    recipient, err
                );

                // Catch recipient validation errors.
                if let MailContextError::Api(err) = &err {
                    match ValidationState::from(err) {
                        ValidationState::InvalidEmail => {
                            return SaveOrSendError::SendMessage(
                                PackageError::RecipientEmailInvalid(recipient.clone()),
                            )
                            .into();
                        }
                        ValidationState::DoesNotExist => {
                            return SaveOrSendError::SendMessage(
                                PackageError::ProtonRecipientDoesNotExist(recipient.clone()),
                            )
                            .into();
                        }
                        ValidationState::Unknown => {
                            return SaveOrSendError::SendMessage(
                                PackageError::RecipientEmailInvalid(recipient.clone()),
                            )
                            .into();
                        }
                        _ => {}
                    }
                }
                err
            })?;
        debug!("{} recipient preferences: {}", recipient, send_preference);
        send_preferences.insert(recipient.clone(), send_preference);
    }
    Ok(send_preferences)
}

#[allow(clippy::too_many_arguments)]
/// Builds the email packages for all recipients.
pub async fn build_packages<Provider: PGPProviderSync>(
    context: &MailUserContext,
    pgp_provider: &Provider,
    address_keys: &UnlockedAddressKeys<Provider>,
    send_preferences: HashMap<String, SendPreferences<Provider::PublicKey>>,
    mime_type: MimeType,
    stored_message_body: &str,
    attachments: &[Attachment],
    guard: &mut WriterGuard<'_>,
) -> Result<Vec<Package>, PackageError> {
    // Which packages do we have to generate?
    let demanded_packages: HashSet<_> = send_preferences
        .values()
        .map(|send_preference| send_preference.mime_type)
        .collect();
    let primary = address_keys
        .primary_for_mail()
        .map_err(|_| PackageError::PrimaryKeyNotFound)?;
    let mut encrypted_packages = Vec::with_capacity(demanded_packages.len());

    for demanded_package in demanded_packages {
        // The options for encrypted content are text, html, or multipart mixed.
        let encrypted_package = match demanded_package {
            PackageMimeType::Html => {
                generate_html_encrypted_package_body(pgp_provider, &primary, stored_message_body)?
            }
            PackageMimeType::Text => generate_text_encrypted_package_body(
                pgp_provider,
                &primary,
                mime_type,
                stored_message_body,
            )?,
            PackageMimeType::Multipart => {
                generate_mime_top_package(
                    context,
                    pgp_provider,
                    &primary,
                    mime_type,
                    stored_message_body,
                    attachments,
                    guard,
                )
                .await?
            }
        };
        encrypted_packages.push(encrypted_package);
    }

    let mut packages = Vec::with_capacity(encrypted_packages.len());
    for encrypted_package in encrypted_packages {
        // For each encrypted package that contains a specific body type.
        // Create the per recipient specific encrypted data.

        // Select the matching send preferences for the package.
        let preferences: Vec<_> = send_preferences
            .iter()
            .filter(|(email, send_preference)| {
                let use_key = encrypted_package.mime_type == send_preference.mime_type;
                if use_key {
                    debug!(
                        "build recipient {} top package for the {} body package",
                        email, encrypted_package.mime_type
                    );
                }
                use_key
            })
            .collect();
        // Build the recipient parts of the package
        let package = build_top_package(
            pgp_provider,
            address_keys,
            &preferences,
            &encrypted_package,
            attachments,
        )?;
        packages.push(package);
    }
    Ok(packages)
}

/// Encrypts an html body.
pub fn generate_html_encrypted_package_body<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    address_key: &PrimaryUnlockedAddressKey<Provider::PrivateKey, Provider::PublicKey>,
    body: &str,
) -> Result<EncryptedPackageBody, PackageError> {
    debug!("Encrypt package for html");
    // No up-convert text is fine
    let package_body = package_body_encrypt(
        pgp_provider,
        address_key,
        PackageMimeType::Html,
        body.as_bytes(),
    )?;
    Ok(package_body)
}

/// Encrypts a text body.
///
/// Converts html to text if necessary.
pub fn generate_text_encrypted_package_body<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    address_key: &PrimaryUnlockedAddressKey<Provider::PrivateKey, Provider::PublicKey>,
    mime_type: MimeType,
    body: &str,
) -> Result<EncryptedPackageBody, PackageError> {
    debug!("Encrypt package for text");
    let text_body: String;
    let body_data = if mime_type == MimeType::TextPlain {
        body
    } else {
        text_body = html_to_text(body);
        &text_body
    };
    let package_body = package_body_encrypt(
        pgp_provider,
        address_key,
        PackageMimeType::Text,
        body_data.as_bytes(),
    )?;
    Ok(package_body)
}

/// Builds and encrypts a pgp/mime message
pub async fn generate_mime_top_package<Provider: PGPProviderSync>(
    context: &MailUserContext,
    pgp_provider: &Provider,
    address_key: &PrimaryUnlockedAddressKey<Provider::PrivateKey, Provider::PublicKey>,
    mime_type: MimeType,
    body: &str,
    attachments: &[Attachment],
    guard: &mut WriterGuard<'_>,
) -> Result<EncryptedPackageBody, PackageError> {
    debug!("Encrypt package for mime");
    let mut content = Vec::with_capacity(body.len());
    let mut builder = InboxMimeBuilder::new();

    // Generate the multipart/mime message body.
    let text_body: String;
    if mime_type == MimeType::TextHtml {
        text_body = html_to_text(body);
        builder = builder.html_body(body).text_body(&text_body);
    } else {
        builder = builder.text_body(body);
    }

    // Load attachments and integrate them into the multipart/mime message.
    // There is no streaming currently.
    for attachment in attachments {
        match attachment.attachment_type {
            crate::models::AttachmentType::Remote(Some(_)) => (),
            crate::models::AttachmentType::Remote(None) => {
                return Err(PackageError::AttachmentNoRemoteId);
            }
            crate::models::AttachmentType::Pgp => {
                continue;
            }
        }

        let loaded_data = attachment
            .content_data(context, guard)
            .instrument(
                debug_span!("mime_package::get_attachment_content_data", id = ?attachment.local_id),
            )
            .await
            .map_err(|e| {
                error!("Failed to read attachment file: {e:?}");
                PackageError::AttachmentLoad(Box::new(e))
            })?;

        let mime_type = attachment.mime_type.to_string();

        match attachment.disposition {
            Disposition::Attachment => {
                builder = builder.attachment(&attachment.filename, Some(mime_type), loaded_data);
            }
            Disposition::Inline => {
                if let Some(content_id) = &attachment.content_id {
                    builder = builder.inline_attachment(
                        content_id.as_str(),
                        &attachment.filename,
                        Some(mime_type),
                        loaded_data,
                    );
                } else {
                    builder =
                        builder.attachment(&attachment.filename, Some(mime_type), loaded_data);
                }
            }
        }
    }
    builder
        .write_to(&mut content)
        .map_err(|err| PackageError::MimeBodyBuild(err.to_string()))?;

    // Encrypt the multipart/mime message.
    let package_body = package_body_encrypt(
        pgp_provider,
        address_key,
        PackageMimeType::Multipart,
        &content,
    )?;
    Ok(package_body)
}

/// Helper function to build a single send request package.
pub fn build_top_package<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    sender_keys: &[UnlockedAddressKey<Provider>],
    recipient_preferences: &[(&String, &SendPreferences<Provider::PublicKey>)],
    encrypted_body: &EncryptedPackageBody,
    attachments: &[Attachment],
) -> Result<Package, PackageError> {
    let mut package = Package {
        body: Some(encrypted_body.encrypted_body.clone().into()),
        mime_type: encrypted_body.mime_type,
        addresses: HashMap::new(),
        package_type: 0,
        body_key: None,
        attachment_keys: None,
    };
    // Build a address sub package for each recipient.
    for (recipient_email, recipient_preferences) in recipient_preferences {
        build_address_sub_package(
            recipient_email,
            &mut package,
            pgp_provider,
            &encrypted_body.session_key,
            attachments,
            sender_keys,
            recipient_preferences,
        )?;
    }

    // The package type is the or of all address sub package types.
    package.package_type = package.addresses.iter().fold(0, |acc, (_, address)| {
        acc | address.address_type.type_value()
    });

    Ok(package)
}

/// Build an address sub package for a recipient and attaches it to the input `top_package`.`
pub fn build_address_sub_package<Provider: PGPProviderSync>(
    recipient_mail: &str,
    top_package: &mut Package,
    pgp_provider: &Provider,
    body_session_key: &InboxSessionKey,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<Provider::PrivateKey>],
    recipient_send_preferences: &SendPreferences<Provider::PublicKey>,
) -> Result<(), PackageError> {
    let mut address_package = AddressSubPackage {
        address_type: recipient_send_preferences.pgp_scheme,
        body_key_packet: None,
        attachment_key_packets: None,
        attachment_enc_signatures: None,
        signature: None,
        token: None,
        enc_token: None,
        auth: None,
        password_hint: None,
    };

    // Based on the encrypt type the recipient wants to receive
    // build the sub-package and modify the top-package.
    match recipient_send_preferences.pgp_scheme {
        PackageCryptoType::ProtonMail | PackageCryptoType::PgpMime => {
            // Encrypt the body session key towards the recipient.
            let recipient_key = recipient_send_preferences
                .selected_key
                .as_ref()
                .ok_or(PackageError::NoRecipientKey)?;
            let recipient_key_packet = body_session_key
                .encrypt_to_recipient(pgp_provider, &recipient_key)
                .map_err(PackageError::PackageBodyInfoReEncrypt)?;
            address_package.body_key_packet = Some(recipient_key_packet);

            // For proton mail we need to re-encrypt attachments towards the recipient.
            // In pgp/mime, they are embedded in the body.
            if recipient_send_preferences.pgp_scheme == PackageCryptoType::ProtonMail {
                process_attachments(
                    &mut address_package,
                    pgp_provider,
                    attachments,
                    sender_keys,
                    recipient_key,
                    recipient_send_preferences.sign,
                )?;
            }
        }
        PackageCryptoType::Cleartext => {
            // Reveal the session key of the body to the server.
            top_package.body_key = Some(body_session_key.to_owned().into());
            // Reveal the session keys of the attachments to the server.
            process_attachment_cleartext(top_package, pgp_provider, attachments, sender_keys)?;
            address_package.signature = Some(PackageSignaturesMode::None);
        }
        PackageCryptoType::ClearMime => {
            // Reveal the session key of the body to the server.
            top_package.body_key = Some(body_session_key.to_owned().into());
            address_package.signature =
                Some(PackageSignaturesMode::from(recipient_send_preferences.sign));
        }
        PackageCryptoType::PgpInline | PackageCryptoType::EncryptedOutside => {
            return Err(PackageError::NotSupported(
                recipient_send_preferences.pgp_scheme,
            ));
        }
    }
    top_package
        .addresses
        .insert(recipient_mail.to_owned(), address_package);
    Ok(())
}

/// Attaches the session keys of all attachments to the top package.
pub fn process_attachment_cleartext<Provider: PGPProviderSync>(
    top_package: &mut Package,
    pgp_provider: &Provider,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<Provider::PrivateKey>],
) -> Result<(), PackageError> {
    if top_package.attachment_keys.is_some() {
        // They are already there from another recipient.
        return Ok(());
    }
    // Reveal session keys of the attachments to the server.
    let mut attachment_keys = HashMap::new();
    for attachment in attachments {
        let remote_attachment_id = match &attachment.attachment_type {
            crate::models::AttachmentType::Remote(Some(id)) => id,
            crate::models::AttachmentType::Remote(None) => {
                //TODO(ET-1407): Correctly handle this error.
                return Err(PackageError::AttachmentNoRemoteId);
            }
            crate::models::AttachmentType::Pgp => {
                continue;
            }
        };

        let attachment_info = attachment.decrypt_attachment_info(pgp_provider, sender_keys)?;
        attachment_keys.insert(
            remote_attachment_id.to_string(),
            attachment_info.session_key.into(),
        );
    }
    if !attachment_keys.is_empty() {
        top_package.attachment_keys = Some(attachment_keys);
    }
    Ok(())
}

/// Encrypts the attachment info (session key, signatures) to the given recipient
/// and adds them to to to the `address_package`.
fn process_attachments<Provider: PGPProviderSync>(
    address_package: &mut AddressSubPackage,
    pgp_provider: &Provider,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<Provider::PrivateKey>],
    recipient_key: &Provider::PublicKey,
    sign: bool,
) -> Result<(), PackageError> {
    let mut attachment_key_packets = HashMap::with_capacity(attachments.len());
    let mut attachment_enc_signatures = HashMap::with_capacity(attachments.len());
    let mut sign = sign;

    // Encrypt the attachment towards the recipient.
    for attachment in attachments {
        let remote_attachment_id = match &attachment.attachment_type {
            crate::models::AttachmentType::Remote(Some(id)) => id,
            crate::models::AttachmentType::Remote(None) => {
                return Err(PackageError::AttachmentNoRemoteId);
            }
            crate::models::AttachmentType::Pgp => {
                continue;
            }
        };

        if attachment.signature.is_none() && attachment.enc_signature.is_none() {
            sign = false;
        }

        // Decrypt attachment information using sender's keys
        let attachment_info = attachment.decrypt_attachment_info(pgp_provider, sender_keys)?;
        // Encrypt the attachment session key to the recipient
        let recipient_attachment_kp = attachment_info
            .encrypt_session_key_to_recipient(pgp_provider, recipient_key)
            .map_err(PackageError::PackageAttachmentInfoReEncrypt)?;

        // Optionally encrypt the signature to the recipient
        if let Some(enc_signature) = attachment_info
            .encrypt_signature_to_recipient(pgp_provider, recipient_key)
            .map_err(PackageError::PackageAttachmentInfoReEncryptSignature)?
        {
            attachment_enc_signatures.insert(
                remote_attachment_id.to_string(),
                enc_signature.encode_base64(),
            );
        }

        attachment_key_packets.insert(remote_attachment_id.to_string(), recipient_attachment_kp);
    }

    if !attachment_key_packets.is_empty() {
        address_package.attachment_key_packets = Some(attachment_key_packets);
    }
    if !attachment_enc_signatures.is_empty() {
        address_package.attachment_enc_signatures = Some(attachment_enc_signatures);
    }
    address_package.signature = Some(PackageSignaturesMode::from(sign));
    Ok(())
}
