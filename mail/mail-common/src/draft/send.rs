mod attachments;

use self::attachments::*;
use crate::datatypes::LocalMessageId;
use crate::datatypes::{Disposition, MimeType};
use crate::draft::recipients::ValidationState;
use crate::draft::{CancelScheduleSendError, PackageError, SendError, compose::html_to_text};
use crate::models::{Attachment, AttachmentType, DraftMetadata, Message};
use crate::{AppError, MailContextError, MailContextResult, MailUserContext};
use anyhow::anyhow;
use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveTime};
use proton_account_api::AccountApi;
use proton_action_queue::observers::ActionAwaiter;
use proton_action_queue::queue::{BroadcastMessage, Queue, QueuedError};
use proton_core_api::consts::Mail;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{PrivateEmail, PrivateEmailRef};
use proton_core_api::session::Session;
use proton_core_common::models::{ModelExtension, PaidSubscription};
use proton_core_common::services::NetworkMonitorService;
use proton_crypto_account::keys::{
    PrimaryUnlockedAddressKey, UnlockedAddressKey, UnlockedAddressKeys,
};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::attachment::DecryptableAttachment;
use proton_crypto_inbox::eo::Challenge;
use proton_crypto_inbox::keys::{
    ComposerPreference, CryptoMailSettings, InboxSessionKey, PackageCryptoType, SendPreferences,
};
use proton_crypto_inbox::message::packages::{
    EncryptedPackageBody, PackageMimeType, package_body_encrypt,
};
use proton_crypto_inbox::proton_crypto::new_srp_provider;
use proton_crypto_inbox::proton_crypto_inbox_mime::write::InboxMimeBuilder;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::prelude::AuthInput;
use proton_mail_api::services::proton::request_data::{
    AddressSubPackage, Package, PackageSignaturesMode,
};
use secrecy::{ExposeSecret, SecretString};
use stash::orm::Model;
use stash::stash::{RunTransaction, Tether};
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::time::Duration;
use tracing::{Instrument, debug, debug_span, error, info, instrument};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailType {
    Draft,
    Direct,
}

/// Encrypt e-mail with password composer input (EO)
pub struct EoData {
    pub password: SecretString,
    pub password_hint: Option<String>,
}

#[instrument(skip_all)]
pub async fn load_prefs<P>(
    context: &MailUserContext,
    pgp: &P,
    tx: &mut impl RunTransaction,
    recipient_emails: &[PrivateEmail],
    crypto_mail_settings: CryptoMailSettings,
    composer_preference: ComposerPreference,
) -> MailContextResult<HashMap<PrivateEmail, SendPreferences<P::PublicKey>>>
where
    P: PGPProviderSync,
{
    let mut send_preferences = HashMap::with_capacity(recipient_emails.len());

    for recipient in recipient_emails {
        let send_preference = context
            .recipient_send_preferences(
                pgp,
                tx,
                PrivateEmailRef::new(recipient.as_clear_text_str()),
                crypto_mail_settings,
                composer_preference,
            )
            .await
            .map_err(|err| {
                error!(
                    "Failed to load send preferences for recipient {}: {}",
                    recipient, err
                );

                if let MailContextError::Api(err) = &err {
                    match ValidationState::from(err) {
                        ValidationState::InvalidEmail => {
                            return SendError::SendMessage(PackageError::RecipientEmailInvalid(
                                recipient.clone(),
                            ))
                            .into();
                        }
                        ValidationState::DoesNotExist => {
                            return SendError::SendMessage(
                                PackageError::ProtonRecipientDoesNotExist(recipient.clone()),
                            )
                            .into();
                        }
                        ValidationState::Unknown => {
                            return SendError::SendMessage(PackageError::RecipientEmailInvalid(
                                recipient.clone(),
                            ))
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
#[instrument(skip_all)]
pub async fn build_packages<P>(
    context: &MailUserContext,
    ty: MailType,
    pgp: &P,
    address_keys: &UnlockedAddressKeys<P>,
    send_preferences: HashMap<PrivateEmail, SendPreferences<P::PublicKey>>,
    mime_type: MimeType,
    stored_message_body: &str,
    attachments: &[Attachment],
    eo_data: Option<EoData>,
    tx: &mut impl RunTransaction,
) -> Result<Vec<Package>, PackageError>
where
    P: PGPProviderSync,
{
    let demanded_packages: HashSet<_> = send_preferences
        .values()
        .map(|send_preference| send_preference.mime_type)
        .collect();

    let primary = address_keys
        .primary_for_mail()
        .map_err(|_| PackageError::PrimaryKeyNotFound)?;

    let mut encrypted_packages = Vec::with_capacity(demanded_packages.len());

    for demanded_package in demanded_packages {
        let encrypted_package = match demanded_package {
            PackageMimeType::Html => {
                generate_html_encrypted_package_body(pgp, &primary, stored_message_body)?
            }
            PackageMimeType::Text => {
                generate_text_encrypted_package_body(pgp, &primary, mime_type, stored_message_body)?
            }
            PackageMimeType::Multipart => {
                generate_mime_top_package(
                    context,
                    pgp,
                    &primary,
                    mime_type,
                    stored_message_body,
                    attachments,
                    tx,
                )
                .await?
            }
        };

        encrypted_packages.push(encrypted_package);
    }

    let mut packages = Vec::with_capacity(encrypted_packages.len());

    for encrypted_package in encrypted_packages {
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

        let package = build_top_package(
            ty,
            pgp,
            address_keys,
            &preferences,
            &encrypted_package,
            attachments,
            eo_data.as_ref(),
            context.session(),
        )
        .await?;

        packages.push(package);
    }

    Ok(packages)
}

#[instrument(skip_all)]
fn generate_html_encrypted_package_body<P>(
    pgp: &P,
    address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    body: &str,
) -> Result<EncryptedPackageBody, PackageError>
where
    P: PGPProviderSync,
{
    debug!("Encrypt package for html");

    // No up-convert text is fine
    let package_body =
        package_body_encrypt(pgp, address_key, PackageMimeType::Html, body.as_bytes())?;

    Ok(package_body)
}

#[instrument(skip_all)]
fn generate_text_encrypted_package_body<P>(
    pgp: &P,
    address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    mime_type: MimeType,
    body: &str,
) -> Result<EncryptedPackageBody, PackageError>
where
    P: PGPProviderSync,
{
    debug!("Encrypt package for text");

    let text_body: String;

    let body_data = if mime_type == MimeType::TextPlain {
        body
    } else {
        text_body = html_to_text(body);
        &text_body
    };

    let package_body = package_body_encrypt(
        pgp,
        address_key,
        PackageMimeType::Text,
        body_data.as_bytes(),
    )?;

    Ok(package_body)
}

#[instrument(skip_all)]
async fn generate_mime_top_package<P>(
    context: &MailUserContext,
    pgp: &P,
    address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    mime_type: MimeType,
    body: &str,
    attachments: &[Attachment],
    tx: &mut impl RunTransaction,
) -> Result<EncryptedPackageBody, PackageError>
where
    P: PGPProviderSync,
{
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
            AttachmentType::Remote(_) => (),
            AttachmentType::Pgp => continue,
        }

        let loaded_data = attachment
            .content_data(context, tx)
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
    let package_body =
        package_body_encrypt(pgp, address_key, PackageMimeType::Multipart, &content)?;

    Ok(package_body)
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
async fn build_top_package<P>(
    ty: MailType,
    pgp: &P,
    sender_keys: &[UnlockedAddressKey<P>],
    recipient_preferences: &[(&PrivateEmail, &SendPreferences<P::PublicKey>)],
    encrypted_body: &EncryptedPackageBody,
    attachments: &[Attachment],
    eo_data: Option<&EoData>,
    session: &Session,
) -> Result<Package, PackageError>
where
    P: PGPProviderSync,
{
    let mut package = Package {
        body: Some(encrypted_body.encrypted_body.clone().into()),
        mime_type: encrypted_body.mime_type,
        addresses: HashMap::new(),
        package_type: 0,
        body_key: None,
        attachment_keys: None,
    };

    for (recipient_email, recipient_preferences) in recipient_preferences {
        build_address_sub_package(
            ty,
            pgp,
            recipient_email,
            &mut package,
            &encrypted_body.session_key,
            attachments,
            sender_keys,
            recipient_preferences,
            eo_data,
            session,
        )
        .await?;
    }

    package.package_type = package.addresses.iter().fold(0, |acc, (_, address)| {
        acc | address.address_type.type_value()
    });

    Ok(package)
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip_all)]
async fn build_address_sub_package<P>(
    ty: MailType,
    pgp: &P,
    recipient_mail: &str,
    top_package: &mut Package,
    body_session_key: &InboxSessionKey,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<P::PrivateKey>],
    recipient_send_preferences: &SendPreferences<P::PublicKey>,
    eo_data: Option<&EoData>,
    session: &Session,
) -> Result<(), PackageError>
where
    P: PGPProviderSync,
{
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
                .encrypt_to_recipient(pgp, &recipient_key)
                .map_err(PackageError::PackageBodyInfoReEncrypt)?;

            address_package.body_key_packet = Some(recipient_key_packet);

            // For proton mail we need to re-encrypt attachments towards the recipient.
            // In pgp/mime, they are embedded in the body.
            if recipient_send_preferences.pgp_scheme == PackageCryptoType::ProtonMail {
                process_attachments(
                    ty,
                    pgp,
                    attachments,
                    sender_keys,
                    EncryptionTool::PublicKey(recipient_key),
                    recipient_send_preferences.sign,
                    &mut address_package,
                )?;
            }
        }

        PackageCryptoType::Cleartext => {
            top_package.body_key = Some(body_session_key.to_owned().into());

            process_attachment_cleartext(ty, pgp, attachments, sender_keys, top_package)?;

            address_package.signature = Some(PackageSignaturesMode::None);
        }

        PackageCryptoType::ClearMime => {
            top_package.body_key = Some(body_session_key.to_owned().into());

            address_package.signature =
                Some(PackageSignaturesMode::from(recipient_send_preferences.sign));
        }

        PackageCryptoType::EncryptedOutside => {
            let Some(eo_info) = eo_data else {
                return Err(PackageError::PackageEoPasswordMissing);
            };

            let response = session
                .get_auth_modulus()
                .await
                .map_err(PackageError::ModulusRequest)?;

            // Re-encrypt packets with the password.
            build_address_package_for_eo(
                pgp,
                eo_info,
                &response.modulus_id,
                &response.modulus,
                body_session_key,
                &mut address_package,
            )?;

            process_attachments(
                ty,
                pgp,
                attachments,
                sender_keys,
                EncryptionTool::Password(eo_info.password.expose_secret()),
                recipient_send_preferences.sign,
                &mut address_package,
            )?;
        }

        PackageCryptoType::PgpInline => {
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

#[instrument(skip_all)]
fn build_address_package_for_eo<P>(
    pgp: &P,
    eo_data: &EoData,
    srp_modulus_id: &str,
    srp_modulus: &str,
    body_session_key: &InboxSessionKey,
    address_package: &mut AddressSubPackage,
) -> Result<(), PackageError>
where
    P: PGPProviderSync,
{
    let srp = new_srp_provider();

    // Auth data.
    let challenge = Challenge::generate(pgp, &srp, eo_data.password.expose_secret(), srp_modulus)?;

    address_package.password_hint = Some(eo_data.password_hint.clone().unwrap_or_default());
    address_package.enc_token = Some(challenge.enc_token);
    address_package.token = Some(challenge.token.deref().to_string());
    address_package.auth = Some(AuthInput {
        version: challenge.verifier.version,
        modulus_id: srp_modulus_id.to_string(),
        salt: challenge.verifier.salt,
        verifier: challenge.verifier.verifier,
    });

    // Body key packet.
    address_package.body_key_packet = Some(
        body_session_key
            .encrypt_to_password(pgp, eo_data.password.expose_secret())
            .map_err(PackageError::PackageBodyInfoReEncrypt)?,
    );
    Ok(())
}

#[instrument(skip_all)]
fn process_attachment_cleartext<P>(
    ty: MailType,
    pgp: &P,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<P::PrivateKey>],
    top_package: &mut Package,
) -> Result<(), PackageError>
where
    P: PGPProviderSync,
{
    if top_package.attachment_keys.is_some() {
        // They are already there from another recipient.
        return Ok(());
    }

    let mut attachment_keys = PackageAttachmentEntries::new(ty);

    for attachment in attachments {
        if attachment.attachment_type.is_pgp() {
            continue;
        }

        if attachment.key_packets.is_none() {
            return Err(PackageError::AttachmentMissingKeyPackets(attachment.id()));
        }

        let attachment_key = attachment
            .decrypt_attachment_info(pgp, sender_keys)?
            .session_key
            .into();

        attachment_keys.insert(attachment, attachment_key)?
    }

    if !attachment_keys.is_empty() {
        top_package.attachment_keys = Some(attachment_keys.into());
    }

    Ok(())
}

enum EncryptionTool<'a, P: PGPProviderSync> {
    Password(&'a str),
    PublicKey(&'a P::PublicKey),
}

/// Encrypts the attachment info (session key, signatures) to the given recipient
/// and adds them to to to the `address_package`.
#[instrument(skip_all)]
fn process_attachments<P>(
    ty: MailType,
    pgp: &P,
    attachments: &[Attachment],
    sender_keys: &[impl AsRef<P::PrivateKey>],
    encryption_tool: EncryptionTool<P, '_>,
    mut sign: bool,
    address_package: &mut AddressSubPackage,
) -> Result<(), PackageError>
where
    P: PGPProviderSync,
{
    let mut attachment_key_packets = PackageAttachmentEntries::new(ty);
    let mut attachment_enc_signatures = PackageAttachmentEntries::new(ty);

    for attachment in attachments {
        if attachment.attachment_type.is_pgp() {
            continue;
        }

        if attachment.signature.is_none() && attachment.enc_signature.is_none() {
            sign = false;
        }

        // Decrypt attachment information using sender's keys
        if attachment.key_packets.is_none() {
            // check if this really set since we assert in the next call.
            return Err(PackageError::AttachmentMissingKeyPackets(attachment.id()));
        }

        let attachment_info = attachment.decrypt_attachment_info(pgp, sender_keys)?;

        let recipient_attachment_kp = match encryption_tool {
            EncryptionTool::Password(password) => {
                // Encrypt the attachment session key to the password
                attachment_info
                    .encrypt_session_key_to_password(pgp, password)
                    .map_err(PackageError::PackageAttachmentInfoReEncrypt)?
            }
            EncryptionTool::PublicKey(recipient_key) => {
                // Encrypt the attachment session key to the recipient
                let recipient_attachment_kp = attachment_info
                    .encrypt_session_key_to_recipient(pgp, recipient_key)
                    .map_err(PackageError::PackageAttachmentInfoReEncrypt)?;

                // Optionally encrypt the signature to the recipient
                if let Some(enc_signature) = attachment_info
                    .encrypt_signature_to_recipient(pgp, recipient_key)
                    .map_err(PackageError::PackageAttachmentInfoReEncryptSignature)?
                {
                    attachment_enc_signatures.insert(attachment, enc_signature.encode_base64())?;
                }
                recipient_attachment_kp
            }
        };

        attachment_key_packets.insert(attachment, recipient_attachment_kp)?;
    }

    if !attachment_key_packets.is_empty() {
        address_package.attachment_key_packets = Some(attachment_key_packets.into());
    }
    if !attachment_enc_signatures.is_empty() {
        address_package.attachment_enc_signatures = Some(attachment_enc_signatures.into());
    }

    address_package.signature = Some(PackageSignaturesMode::from(sign));

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("An invalid date time was generated")]
pub struct ScheduleSendOptionsDateTimeError;

pub struct ScheduleSendOptions<Tz: chrono::TimeZone> {
    /// Timestamp for the next day at 8:00
    pub time_tomorrow: DateTime<Tz>,
    /// Timestamp for the next Monday at 8:00
    pub time_next_monday: DateTime<Tz>,
    /// Indicates whether the custom date time picker is available, paying users only.
    pub is_custom_datetime_available: bool,
}

impl ScheduleSendOptions<Local> {
    pub fn new(
        user_subscriptions: PaidSubscription,
    ) -> Result<Self, ScheduleSendOptionsDateTimeError> {
        let now = Local::now();
        Ok(Self {
            time_tomorrow: Self::calculate_tomorrow(now)?,
            time_next_monday: Self::calculate_next_monday(now)?,
            is_custom_datetime_available: user_subscriptions.contains(PaidSubscription::MAIL),
        })
    }
}

impl<Tz: chrono::TimeZone> ScheduleSendOptions<Tz> {
    fn calculate_tomorrow(
        now: DateTime<Tz>,
    ) -> Result<DateTime<Tz>, ScheduleSendOptionsDateTimeError> {
        Self::calculate_next(now, 1)
    }
    fn calculate_next_monday(
        now: DateTime<Tz>,
    ) -> Result<DateTime<Tz>, ScheduleSendOptionsDateTimeError> {
        let days = 7 - now.weekday().num_days_from_monday();
        Self::calculate_next(now, days as u64)
    }

    pub(super) fn calculate_next(
        now: DateTime<Tz>,
        days: u64,
    ) -> Result<DateTime<Tz>, ScheduleSendOptionsDateTimeError> {
        let Some(tomorrow) = now.checked_add_days(Days::new(days)) else {
            error!("Failed to calculate next date");
            return Err(ScheduleSendOptionsDateTimeError);
        };

        match tomorrow.with_time(NaiveTime::from_hms_opt(8, 0, 0).expect("Should never fail")) {
            LocalResult::Single(v) => Ok(v),
            LocalResult::Ambiguous(v1, _) => Ok(v1),
            LocalResult::None => {
                error!("Failed to calculate date time at 08:00");
                Err(ScheduleSendOptionsDateTimeError)
            }
        }
    }
}

/// Attempt to cancel a schedule send.
///
/// Contrary to all other methods that operate on the server, this method does not queue any actions
/// as we need to guarantee that the cancel request reaches the servers at the time it is performed.
///
/// Failing to do so can lead to cancellation request running after the message has already been
/// sent.
///
/// On completion returns the original scheduled time of the message.
#[instrument(
    level = "debug",
    skip(
        tether,
        queue,
        session,
        wait_on_completion_duration,
        network_monitor_service
    )
)]
pub async fn cancel_schedule_send(
    message_id: LocalMessageId,
    tether: &mut Tether,
    queue: &Queue,
    session: &Session,
    wait_on_completion_duration: Duration,
    network_monitor_service: &NetworkMonitorService,
) -> Result<DateTime<Local>, MailContextError> {
    info!("Cancelling schedule sent message");
    // Validate if the message is actually scheduled for sending
    let message = Message::find_by_id(message_id, tether)
        .await?
        .ok_or(CancelScheduleSendError::MessageNotFound(message_id))?;

    if !message.is_scheduled_for_send() {
        return Err(CancelScheduleSendError::MessageIsNotScheduled(message_id).into());
    }

    // Pre-check we are offline before proceeding to avoid long stalls during api requests.
    if network_monitor_service.check_now().await.is_offline() {
        return Err(MailContextError::Api(ApiServiceError::NetworkError(
            "Offline".into(),
        )));
    }

    let original_dt: DateTime<Local> = message
        .time
        .to_date_time()
        .ok_or(MailContextError::Other(anyhow!("Invalid timestamp")))?;

    // If we have metadata for this message it means we created the message and
    // there may still be a queued send request. If we do not have any metadata, it either means
    // the message was scheduled on the server already or by another session.
    let message = if let Some(metadata) =
        DraftMetadata::find_by_message_id(message_id, tether).await?
    {
        debug!("Found metadata, message was sent by us.");
        if let Some(send_action_id) = metadata.send_action_id {
            match queue.cancel(send_action_id).await {
                Ok(_) => {
                    // action was cancelled and state reverted.
                    info!("Message {message_id} schedule send cancelled successfully");
                    return Ok(original_dt);
                }
                Err(QueuedError::ActionNotFound(_)) => {
                    // action already executed, proceed to next stage. Before that we need to
                    // reload the message to check whether it actually succeeded or not.
                    debug!("Action no longer exist, either it succeeded or failed");
                }
                Err(QueuedError::ActionInExecution(id)) => {
                    debug!(
                        "Action is being executed ({id}), waiting at most {wait_on_completion_duration:?} until finished"
                    );
                    // Action is currently being executed, wait for it to finish.
                    let mut waiter = ActionAwaiter::new(queue);

                    let Ok(message) =
                        tokio::time::timeout(wait_on_completion_duration, waiter.wait(id))
                            .await
                            .map_err(|_| CancelScheduleSendError::TimedOut)?
                    else {
                        return Err(MailContextError::Other(anyhow!("Connection to queue lost")));
                    };
                    // If the action did not complete it means this message was not scheduled.
                    if !matches!(message, BroadcastMessage::Success(_, _)) {
                        debug!("Action did not complete successfully");
                        return Err(
                            CancelScheduleSendError::MessageIsNotScheduled(message_id).into()
                        );
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }

        // reload the message in case something changed
        let message = Message::find_by_id(message_id, tether)
            .await?
            .ok_or(CancelScheduleSendError::MessageNotFound(message_id))?;
        if !message.is_scheduled_for_send() {
            return Err(CancelScheduleSendError::MessageIsNotScheduled(message_id).into());
        }
        message
    } else {
        message
    };

    let original_dt: DateTime<Local> = message
        .time
        .to_date_time()
        .ok_or(MailContextError::Other(anyhow!("Invalid timestamp")))?;
    debug!("Cancelling send on the server");
    let remote_id = message
        .remote_id
        .clone()
        .ok_or(AppError::MessageHasNoRemoteId(message_id))?;

    // Invoke server call
    let response = match session.cancel_send(remote_id).await.inspect_err(|e| {
        error!("Failed to cancel send on server: {e:?}");
    }) {
        Ok(response) => response,
        Err(err) => {
            if let Some(api_error) = err.to_proton_error()
                && api_error.code == Mail::MessageAlreadySent as u32
            {
                return Err(CancelScheduleSendError::AlreadySent(message_id).into());
            }
            return Err(err.into());
        }
    };

    // Put message back into drafts
    let mut updated_message = Message::from_api_metadata(response.message, tether).await?;

    tether.tx(async |tx| updated_message.save(tx).await).await?;

    Ok(original_dt)
}

#[cfg(test)]
mod tests {
    // All test need to run with utc timezone, but the real code should use local timezone.
    use super::*;
    use chrono::Utc;
    use test_case::test_case;

    #[test]
    fn calculate_tomorrow() {
        let now: DateTime<Utc> = DateTime::parse_from_rfc2822("Mon, 12 May 2025 09:30:00 GMT")
            .unwrap()
            .into();
        let expected: DateTime<Utc> = DateTime::parse_from_rfc2822("Tue, 13 May 2025 08:00:00 GMT")
            .unwrap()
            .into();

        let output = ScheduleSendOptions::calculate_tomorrow(now).unwrap();
        assert_eq!(output, expected);
    }

    #[test_case("Mon, 12 May 2025 09:30:00 GMT", "Mon, 19 May 2025 08:00:00 GMT"; "monday to monday" )]
    #[test_case("Wed, 14 May 2025 12:30:00 GMT", "Mon, 19 May 2025 08:00:00 GMT"; "wednesday to monday" )]
    #[test_case("Sun, 18 May 2025 23:30:00 GMT", "Mon, 19 May 2025 08:00:00 GMT"; "Sunday to monday" )]
    #[test_case("Sun, 30 Mar 2025 23:30:00 GMT", "Mon, 31 Mar 2025 08:00:00 GMT"; "Daylight Savings" )]
    fn calculate_next_monday(input: &str, expected: &str) {
        let now: DateTime<Utc> = DateTime::parse_from_rfc2822(input).unwrap().into();
        let expected: DateTime<Utc> = DateTime::parse_from_rfc2822(expected).unwrap().into();

        let output: DateTime<Utc> = ScheduleSendOptions::calculate_next_monday(now).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn send_options_custom_option_available_only_for_paid_users() {
        let options = ScheduleSendOptions::new(PaidSubscription::empty()).unwrap();
        assert!(!options.is_custom_datetime_available);
        let options = ScheduleSendOptions::new(PaidSubscription::MAIL).unwrap();
        assert!(options.is_custom_datetime_available)
    }
}
