use crate::actions::draft;
use crate::actions::draft::{
    AttachmentDispositionUpdate, AttachmentRemove, AttachmentUpload, AttachmentUploadMode, Discard,
    SHARE_EXT_ACTION_GROUP, Save, UndoSend,
};
use crate::datatypes::attachment::{CombinedAttachmentDisposition, ContentId};
use crate::datatypes::{Disposition, LocalAttachmentId, LocalConversationId, LocalMessageId};
use crate::decrypted_message::{DecryptedMessageBody, ThemeOpts};
use crate::draft::attachments::{DraftAttachment, build_attachment_key_packets};
use crate::draft::compose::{
    DraftAddressChangeOutput, DraftAddressChangeRequest, DraftAddressValidationResult,
    PM_SIGNATURE_DIV_CLASS, draft_sender_addresses, encrypt_draft_body, get_alias_component,
    get_full_signature, inject_dark_mode, maybe_sanitize, patch_draft_with_reply_mode,
    prepare_html_reply, prepare_text_reply, resolve_sender_alias, validate_sender_address,
};
use crate::draft::recipients::{ContactGroupResolver, ProtonContactGroupResolver, RecipientList};
use crate::draft::{
    AttachmentDispositionSwapError, AttachmentUploadError, DraftExpirationTime, DraftSyncStatus,
    EoData, Error, ExpirationError, MIN_EXPIRATION_TIME_SECONDS, MIN_PASSWORD_LEN, OpenError,
    PasswordError, ReplyMode, SaveError, ScheduleSendOptions, SendError, SenderAddressChangeError,
    compose, send,
};
use crate::models::{
    Attachment, AttachmentData, AttachmentType, CustomSettings, DraftAttachmentMetadata,
    DraftAttachmentUploadState, DraftMetadata, DraftSendResult, DraftSendResultOrigin,
    MailSettings, Message, MessageMimeType, MetadataId,
};
use crate::{AppError, ImagePolicy, MailContextError, MailContextResult, MailUserContext};
use anyhow::{Context, anyhow};
use chrono::{DateTime, Local};
use futures::future::join3;
use proton_action_queue::action::{ActionId, MetadataBuilder};
use proton_action_queue::queue::{ActionError, Queue, QueuedActionOutput, QueuedError};
use proton_canonical_email::canonicalize_auto;
use proton_core_api::consts::Mail;
use proton_core_api::services::proton::AddressId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::db::account::EncryptedPassword;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension, User};
use proton_core_common::{Origin, Platform};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::DraftReplyOrForwardParams;
use proton_mail_api::services::proton::response_data::Message as ApiMessage;
use proton_mail_html_transformer::Transformer;
use proton_mail_html_transformer::transforms::styles::BrowserCapabilities;
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Represent a new message that is being drafted.
///
/// When creating a new draft, empty or reply, we calculate what the
/// new draft should look like, but we never save it to disk until
/// the user calls [`save()`].
///
/// Since there is associated metadata with these operations, we create
/// a new [`DraftMetadata`] structure whenever we open or create a draft
/// so we can track auxiliary data such as the message id.
///
/// This metadata is kept alive as long as the message it references is alive
/// or the draft is discarded/deleted.
#[derive(derive_more::Debug)]
pub struct Draft {
    pub metadata_id: MetadataId,
    pub sender: String, // email address
    pub to_list: RecipientList,
    pub cc_list: RecipientList,
    pub bcc_list: RecipientList,
    pub address_id: AddressId,
    pub subject: String,
    pub send_result: Option<DraftSendResult>,

    #[debug(skip)]
    body: String,
    mime_type: MessageMimeType,

    /// This is only present when we detect the choice of address in the draft
    /// does not satisfy some requirements that would prevent the message from being sent.
    pub address_validation_result: Option<DraftAddressValidationResult>,

    /// This is only set when creating a reply and is only valid while the instance
    /// of the draft is open after it has been opened or a rew reply has been created.
    sender_alias: Option<String>,

    last_draft_save_action_id: Option<ActionId>,

    /// Wether this is a bring your own email address
    pub is_byoe: bool,
}

impl Draft {
    pub async fn sender_addresses(&self, tether: &Tether) -> Result<Vec<Address>, StashError> {
        draft_sender_addresses(self.sender_alias.as_ref(), &self.address_id, tether).await
    }

    // Note: this only exists for the TUI. Will be removed in the draft refactor.
    pub fn sender_addresses_deferred(&self) -> DraftSenderAddressesDeferred {
        DraftSenderAddressesDeferred {
            sender_alias: self.sender_alias.clone(),
            address_id: self.address_id.clone(),
        }
    }

    pub async fn schedule_send_options(
        ctx: &MailUserContext,
    ) -> MailContextResult<ScheduleSendOptions<Local>> {
        let user = ctx.user().await?;

        ScheduleSendOptions::new(user.subscribed)
            .context("Failed to get schedule send options")
            .map_err(MailContextError::Other)
    }

    #[tracing::instrument(skip(context))]
    pub async fn open(
        context: &MailUserContext,
        message_id: LocalMessageId,
    ) -> Result<(Self, DraftSyncStatus), MailContextError> {
        info!("Opening draft");

        let tether = &mut context.user_stash().connection().await?;

        let Some(mut message) = Message::find_by_id(message_id, tether).await? else {
            error!("Opened message as draft that does not exist.");
            return Err(AppError::MessageMissing(message_id).into());
        };

        if message.deleted {
            return Err(AppError::MessageMissing(message_id).into());
        }

        if !message.is_draft() {
            error!("Opened a non-draft message as a draft");

            return Err(OpenError::MessageNotADraft(message_id).into());
        }

        let mut metadata = if let Some(metadata) =
            DraftMetadata::find_by_message_id(message.local_id.unwrap(), tether)
                .await
                .inspect_err(|e| error!("Failed to load draft metadata: {e:?}"))?
        {
            debug!("Found existing metadata with id {}", metadata.id.unwrap());
            metadata
        } else {
            debug!("No metadata found, creating new entry");

            let mut metadata =
                DraftMetadata::with_ids(message.id(), message.local_conversation_id.unwrap());

            tether
                .tx::<_, _, MailContextError>(async |tx| {
                    metadata
                        .save(tx)
                        .await
                        .inspect_err(|e| error!("Failed to create new metadata: {e:?}"))?;

                    fs::create_dir_all(draft_attachment_staging_path(
                        context,
                        metadata.id.unwrap(),
                    ))
                    .await
                    .inspect_err(|e| error!("Failed to create attachment staging path: {e:?}"))?;

                    Ok(())
                })
                .await?;

            metadata
        };

        // First let's try to sync the body and metadata. If we can't we will fill it
        // ourselves.
        let (decrypted, sync_status) = if metadata.has_pending_changes(tether).await? {
            // If we have pending changes we should not sync the data from the server
            // as that will override local state.
            info!("Draft metadata has pending changes, sync skipped.");
            (None, DraftSyncStatus::Synced)
        } else if let Some(remote_id) = message.remote_id.clone() {
            info!("Draft metadata has no pending changes, syncing.");

            match Message::force_sync_message_and_body(context, remote_id, true, tether).await {
                Ok((message_new, decrypted)) => {
                    message = message_new;

                    debug!("Message synced, updating attachment metadata.");

                    tether
                        .tx(async |tx| {
                            DraftAttachmentMetadata::reset_draft_attachments_after_sync(
                                metadata.id.unwrap(),
                                &decrypted.metadata,
                                tx,
                            )
                            .await?;

                            metadata.set_expiration_time(DraftExpirationTime::Never);
                            metadata.password = None;
                            metadata.password_hint = None;

                            metadata.save(tx).await
                        })
                        .await?;

                    (Some(decrypted), DraftSyncStatus::Synced)
                }

                Err(MailContextError::Api(api_err))
                    if api_err.is_network_failure() || api_err.is_server_failure() =>
                {
                    debug!("Failed to sync draft due to network/service error.");
                    (None, DraftSyncStatus::Cached)
                }

                Err(e) => return Err(e),
            }
        } else {
            debug!("Message does not have a remote id.");

            // If we have no remote id do not return cached status. As this implies the
            // draft was created locally and the save action has not yet executed.
            // We only trigger this code path if the save action failed to execute.
            (None, DraftSyncStatus::Synced)
        };

        let decrypted = match decrypted {
            Some(d) => d,

            None => {
                debug!("Failed to sync draft from server, attempting to load from cache.");

                let Some(d) = Message::load_decrypted_message_from_cache(
                    context.as_arc(),
                    message.id(),
                    &message.remote_address_id,
                    tether,
                )
                .await
                .inspect_err(|e| error!("Failed to load decrypted data from cache: {e:?}"))?
                else {
                    return Err(OpenError::MessageBodyMissing(message.local_id.unwrap()).into());
                };

                d
            }
        };

        let send_result = DraftSendResult::find_by_id(message.local_id.unwrap(), tether)
            .await
            .inspect_err(|e| error!("Failed to load send result: {e:?}"))?;

        let contact_group_resolver = ProtonContactGroupResolver::new(tether);

        let (to_list, cc_list, bcc_list) = join3(
            RecipientList::from_message_recipients(&contact_group_resolver, message.to_list.value),
            RecipientList::from_message_recipients(&contact_group_resolver, message.cc_list.value),
            RecipientList::from_message_recipients(&contact_group_resolver, message.bcc_list.value),
        )
        .await;

        let sender_alias = get_alias_component(message.sender.address.as_clear_text_str())
            .map(|_| message.sender.address.as_clear_text_str().to_owned());

        let address = Address::find_by_remote_id(message.remote_address_id.clone(), tether)
            .await?
            .ok_or(OpenError::AddressNotFound(
                message.remote_address_id.clone(),
            ))?;

        let mut draft = Self {
            metadata_id: metadata.id.unwrap(),
            sender: message.sender.address.into_clear_text_string(),
            to_list,
            cc_list,
            bcc_list,
            address_id: message.remote_address_id,
            subject: message.subject,
            send_result,
            body: decrypted.body,
            mime_type: decrypted.mime_type,
            address_validation_result: None,
            sender_alias,
            last_draft_save_action_id: metadata.save_action_id,
            is_byoe: address.is_byoe(),
        };

        draft.sanitize_body();

        // When syncing the draft from the server  we need to re-check address validity in
        // case something changes.
        if sync_status == DraftSyncStatus::Synced {
            let user = context.user().await?;

            if let Some(result) = validate_sender_address(&address, &user) {
                warn!(
                    "Address {} is no longer valid: {}",
                    result.email, result.error
                );

                let new_address = context
                    .user_context()
                    .address_service()
                    .find_valid_sender_address()
                    .await?
                    .ok_or(OpenError::UserHasNoAddresses)?;

                info!("Draft address changed to {}", new_address.email);

                draft.address_id = new_address.remote_id.unwrap();
                draft.address_validation_result = Some(result);
                draft.sender_alias = None;

                // If this operation fails we should not prevent the draft from being opened, some things
                // may not be correct (signature, public key attachments) but the draft will still be usable.
                if let Err(e) = draft
                    .change_sender_address_by_id(
                        context,
                        new_address.email,
                        draft.address_id.clone(),
                    )
                    .await
                {
                    error!("Failed to change sender address: {e}")
                }
            }
        }

        info!("Draft loaded with id = {}", draft.metadata_id);

        Ok((draft, sync_status))
    }

    #[tracing::instrument(skip_all)]
    pub async fn empty(context: &MailUserContext) -> Result<Self, MailContextError> {
        info!("Creating new empty draft");

        let mut tether = context.user_stash().connection().await?;

        let user = context.user().await?;

        let address = context
            .user_context()
            .address_service()
            .find_valid_sender_address()
            .await?
            .ok_or(OpenError::UserHasNoAddresses)
            .inspect_err(|_| error!("No suitable address found"))?;

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let custom_settings = CustomSettings::get_or_default(&tether).await?;

        let metadata = tether
            .tx::<_, _, MailContextError>(async |tx| {
                let metadata = DraftMetadata::empty(tx)
                    .await
                    .inspect_err(|e| error!("Failed to create new empty draft metadata: {e:?}"))?;

                if mail_settings.attach_public_key {
                    let public_key_attachment =
                        Attachment::create_public_key(context, &address, tx)
                            .await
                            .inspect_err(|e| {
                                error!("Failed to create public key attachment: {e:?}")
                            })?;

                    DraftAttachmentMetadata::pending(
                        metadata.id.unwrap(),
                        public_key_attachment.local_id.unwrap(),
                        0,
                        true,
                    )
                    .save(tx)
                    .await?
                }
                Ok(metadata)
            })
            .await?;

        info!("New draft created with id = {}", metadata.id.unwrap());

        Ok(Self::new_empty_draft(
            metadata.id.unwrap(),
            &user,
            &address,
            &mail_settings,
            &custom_settings,
        ))
    }

    pub(super) fn new_empty_draft(
        metadata_id: MetadataId,
        user: &User,
        address: &Address,
        mail_settings: &MailSettings,
        custom_settings: &CustomSettings,
    ) -> Self {
        let mime_type = MessageMimeType::from_api(mail_settings.draft_mime_type, || {
            unreachable!("draftMimeType cannot be set to multipart/mixed")
        });

        let body = compose::get_full_signature(
            user,
            address,
            mail_settings,
            custom_settings,
            mime_type,
            Platform::current(),
        );

        Self {
            metadata_id,
            sender: address.email.clone(),
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            send_result: None,
            mime_type,
            body,
            address_validation_result: None,
            sender_alias: None,
            last_draft_save_action_id: None,
            is_byoe: address.is_byoe(),
        }
    }

    /// Create a draft as reply/forward to an existing message with `message_id`.
    ///
    /// `use_utc` controls whether we should generate the sender reply using
    /// the `Utc` or `Local` timezone. For production, we should use the `Local`
    /// but for testing in CI `Utc` is more deterministic.
    #[tracing::instrument(skip(context, use_utc))]
    pub async fn reply(
        context: &MailUserContext,
        message_id: LocalMessageId,
        reply_mode: ReplyMode,
        use_utc: bool,
    ) -> Result<Self, MailContextError> {
        info!("Creating new draft reply");

        let mut tether = context.user_stash().connection().await?;

        let Some(source_message) = Message::find_by_id(message_id, &tether).await? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        if source_message.flags.is_draft() {
            return Err(OpenError::ReplyOrForwardToDraft(message_id).into());
        }

        if source_message.remote_id.is_none() {
            return Err(AppError::MessageHasNoRemoteId(message_id).into());
        }

        let user = context.user().await?;

        let address = Address::find_by_remote_id(source_message.remote_address_id.clone(), &tether)
            .await?
            .ok_or(OpenError::AddressNotFound(
                source_message.remote_address_id.clone(),
            ))?;

        let (address, address_validation_error) =
            if let Some(e) = validate_sender_address(&address, &user) {
                warn!("Sender address ({}) is not valid: {}", e.email, e.error);

                let addr = context
                    .user_context()
                    .address_service()
                    .find_valid_sender_address()
                    .await?
                    .ok_or(OpenError::UserHasNoAddresses)
                    .inspect_err(|_| error!("Failed to locate default sender address"))?;

                info!("Sender address changed to: {}", addr.email);

                (addr, Some(e))
            } else {
                (address, None)
            };

        let source_message_body = match source_message
            .fetch_message_body(context, &mut tether)
            .await
        {
            Ok(body) => body,

            Err(err) => {
                error!(?err, "Couldn't get source message");

                return Err(OpenError::MessageBodyMissing(message_id).into());
            }
        };

        let mail_settings = MailSettings::get(&tether).await?.unwrap_or_default();
        let custom_settings = CustomSettings::get_or_default(&tether).await?;

        let expiration_time = if source_message.expiration_time.as_u64() != 0 {
            let delta = source_message
                .expiration_time
                .saturating_sub(source_message.time.as_u64());
            Some(UnixTimestamp::now().saturating_add(delta.as_u64()))
        } else {
            None
        };

        let draft = tether
            .tx::<_, _, MailContextError>(async |tx| {
                let metadata = DraftMetadata::reply(
                    reply_mode,
                    source_message.local_id.unwrap(),
                    source_message.local_conversation_id.unwrap(),
                    expiration_time,
                    tx,
                )
                .await
                .inspect_err(|e| error!("Failed to create new reply draft metadata: {e:?}"))?;

                fs::create_dir_all(draft_attachment_staging_path(context, metadata.id.unwrap()))
                    .await
                    .inspect_err(|e| error!("Failed to create attachment staging path: {e:?}"))?;

                let contact_group_resolver = ProtonContactGroupResolver::new(tx);

                let (draft, attachments) = Self::new_draft_reply(
                    &contact_group_resolver,
                    metadata.id.unwrap(),
                    reply_mode,
                    &user,
                    &address,
                    &mail_settings,
                    &custom_settings,
                    &source_message,
                    source_message_body,
                    use_utc,
                    address_validation_error,
                )
                .await;

                if mail_settings.attach_public_key {
                    let public_key_attachment =
                        Attachment::gen_public_key(context, &address, tx).await?;

                    // If we already have the public key, we should just skip adding the attachment.
                    if !attachments.iter().any(|attachment| {
                        attachment.filename == public_key_attachment.attachment.filename
                    }) {
                        let attachment = public_key_attachment.store(context, tx).await?;

                        DraftAttachmentMetadata::pending(
                            metadata.id.unwrap(),
                            attachment.local_id.unwrap(),
                            0,
                            true,
                        )
                        .save(tx)
                        .await?;
                    }
                }

                for (order, attachment) in attachments.into_iter().enumerate() {
                    let mut attachment_metadata =
                        if matches!(attachment.attachment_type, AttachmentType::Pgp) {
                            // PGP attachments need to be cloned and uploaded to the server so it can be sent.
                            debug!("Cloning PGP attachment {} ", attachment.local_id.unwrap());

                            let new_attachment = Attachment::clone_attachment(
                                context,
                                address.remote_id.clone().unwrap(),
                                attachment,
                                tx,
                            )
                            .await
                            .inspect_err(|e| error!("Failed to clone pgp attachment: {e:?}",))?;

                            debug!(
                                "PGP attachment cloned as {} ",
                                new_attachment.local_id.unwrap()
                            );

                            DraftAttachmentMetadata::pending(
                                metadata.id.unwrap(),
                                new_attachment.local_id.unwrap(),
                                order,
                                false,
                            )
                        } else {
                            DraftAttachmentMetadata::inherited(
                                metadata.id.unwrap(),
                                &attachment,
                                order,
                            )
                        };

                    attachment_metadata
                        .save(tx)
                        .await
                        .inspect_err(|e| error!("Failed to save attachment metadata: {e:?}"))?
                }
                Ok(draft)
            })
            .await?;

        info!("New draft created with id = {}", draft.metadata_id);

        Ok(draft)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn new_draft_reply(
        contact_group_resolver: &impl ContactGroupResolver,
        metadata_id: MetadataId,
        reply_mode: ReplyMode,
        user: &User,
        address: &Address,
        mail_settings: &MailSettings,
        custom_settings: &CustomSettings,
        source_message: &Message,
        mut source_message_body: DecryptedMessageBody,
        use_utc: bool,
        address_validation_result: Option<DraftAddressValidationResult>,
    ) -> (Self, Vec<Attachment>) {
        let mime_type = source_message_body.mime_type;

        let mut body = get_full_signature(
            user,
            address,
            mail_settings,
            custom_settings,
            mime_type,
            Platform::current(),
        );

        match mime_type {
            MessageMimeType::TextHtml => {
                prepare_html_reply(
                    &mut body,
                    source_message,
                    &source_message_body.body,
                    use_utc,
                );
            }
            MessageMimeType::TextPlain => {
                prepare_text_reply(
                    &mut body,
                    source_message,
                    &source_message_body.body,
                    use_utc,
                );
            }
        }

        let mut attachments = std::mem::take(&mut source_message_body.metadata.attachments);

        if reply_mode != ReplyMode::Forward {
            attachments.retain(|attachment| attachment.disposition == Disposition::Inline);
        };

        // Only resolve alias if we passed the validation. If we didn't we are not on the same
        // address.
        let (sender_email, sender_alias) = if address_validation_result.is_none() {
            let new_sender = resolve_sender_alias(&address.email, &source_message_body.metadata);
            let alias = if new_sender != address.email {
                Some(new_sender.clone())
            } else {
                None
            };
            (new_sender, alias)
        } else {
            (address.email.clone(), None)
        };

        let mut draft = Self {
            metadata_id,
            sender: sender_email,
            to_list: RecipientList::new(),
            cc_list: RecipientList::new(),
            bcc_list: RecipientList::new(),
            address_id: address.remote_id.clone().unwrap(),
            subject: String::new(),
            send_result: None,
            body,
            mime_type,
            address_validation_result,
            sender_alias,
            last_draft_save_action_id: None,
            is_byoe: address.is_byoe(),
        };

        patch_draft_with_reply_mode(
            contact_group_resolver,
            &mut draft,
            source_message,
            &source_message_body.metadata,
            reply_mode,
        )
        .await;

        draft.sanitize_body();

        (draft, attachments)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn remote_create(
        context: &MailUserContext,
        session: &Session,
        address_id: AddressId,
        save_action: &Save,
        attachments: &[Attachment],
        message_body: &str,
        draft_reply_or_forward_params: Option<DraftReplyOrForwardParams>,
        tether: &Tether,
    ) -> Result<(ApiMessage, Vec<u8>), MailContextError> {
        let (encrypted, signatures) =
            encrypt_draft_body(context, &address_id, message_body).await?;
        let params = save_action.crate_draft_params(encrypted);

        let force_re_encrypt = draft_reply_or_forward_params.is_some();
        let attachment_key_packets = build_attachment_key_packets(
            context,
            &address_id,
            attachments,
            force_re_encrypt,
            tether,
        )
        .await?;

        let response = session
            .create_draft(
                params,
                attachment_key_packets,
                draft_reply_or_forward_params,
            )
            .await?;

        Ok((response.message, signatures))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn remote_update(
        context: &MailUserContext,
        session: &Session,
        address_id: AddressId,
        local_message_id: LocalMessageId,
        message_id: MessageId,
        save_action: &Save,
        attachments: &[Attachment],
        message_body: &str,
        tether: &Tether,
    ) -> Result<(ApiMessage, Vec<u8>), MailContextError> {
        let (encrypted, signatures) =
            encrypt_draft_body(context, &address_id, message_body).await?;
        let params = save_action.crate_draft_params(encrypted);

        let attachment_key_packets =
            build_attachment_key_packets(context, &address_id, attachments, false, tether).await?;

        match session
            .update_draft(message_id, params, attachment_key_packets)
            .await
        {
            Err(e) => {
                if let Some(proton_error) = e.to_proton_error() {
                    if proton_error.code == Mail::MessageAlreadySent as u32 {
                        return Err(SaveError::AlreadySent.into());
                    } else if proton_error.code == Mail::MessageUpdateDraftNotDraft as u32 {
                        return Err(SaveError::MessageNotADraft(local_message_id).into());
                    } else if proton_error.code == Mail::MessageUpdateDraftNotExist as u32 {
                        return Err(SaveError::DraftDoesNotExistOnServer.into());
                    }
                }

                Err(e.into())
            }

            Ok(response) => Ok((response.message, signatures)),
        }
    }

    pub async fn save(
        &mut self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Save>, MailContextError> {
        let r = self.to_save_action().queue(queue, tether, origin).await;
        if let Ok(output) = &r {
            self.last_draft_save_action_id = Some(output.id)
        }
        r
    }

    pub async fn send(
        &mut self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.to_send_action()?
            .queue(queue, tether, origin, &mut self.last_draft_save_action_id)
            .await
    }

    /// Apply an action which will schedule a send this draft at the given `delivery_time`.
    ///
    /// Note that due to offline mode we will only send this message if at the time we are
    /// executing the request, there is still enough time left to schedule the send.
    pub async fn schedule_send(
        &mut self,
        delivery_time: DateTime<Local>,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        self.to_schedule_send_action(delivery_time)?
            .queue(queue, tether, origin, &mut self.last_draft_save_action_id)
            .await
    }

    pub async fn discard(
        &self,
        queue: &Queue,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        Ok(self.to_discard_action().queue(queue, origin).await?)
    }

    /// Discard a draft with the given `message_id`.
    ///
    /// This is functionally equivalent to [`Draft::discard()`] but does not
    /// require an instance of the [`Draft`] type.
    pub async fn action_discard(
        message_id: LocalMessageId,
        tether: &Tether,
        queue: &Queue,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Discard>, MailContextError> {
        let Some(metadata) = DraftMetadata::find_by_message_id(message_id, tether).await? else {
            return Err(Error::Open(OpenError::MessageNotADraft(message_id)).into());
        };

        Ok(
            DraftDiscardActionQueuer::new(metadata.id.unwrap(), Discard::new(metadata.id.unwrap()))
                .queue(queue, origin)
                .await?,
        )
    }

    fn to_save_action(&self) -> DraftSaveActionQueuer {
        DraftSaveActionQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            Save::new(self, DraftSendResultOrigin::Save),
            self.last_draft_save_action_id,
        )
    }

    fn to_send_action(&self) -> Result<DraftSendActionQueuer, Error> {
        self.to_send_action_impl(None)
    }

    fn to_schedule_send_action(
        &self,
        delivery_time: DateTime<Local>,
    ) -> Result<DraftSendActionQueuer, Error> {
        self.to_send_action_impl(Some(delivery_time))
    }

    fn to_send_action_impl(
        &self,
        delivery_time: Option<DateTime<Local>>,
    ) -> Result<DraftSendActionQueuer, Error> {
        if self.to_list.is_empty() && self.cc_list.is_empty() && self.bcc_list.is_empty() {
            return Err(SendError::NoRecipients.into());
        }

        let save_action = DraftSaveActionQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            Save::new(self, DraftSendResultOrigin::SaveBeforeSend),
            self.last_draft_save_action_id,
        );

        let send_action = if let Some(delivery_time) = delivery_time {
            draft::Send::scheduled(self, delivery_time)
        } else {
            draft::Send::new(self)
        };

        Ok(DraftSendActionQueuer::new(
            self.metadata_id,
            save_action,
            send_action,
        ))
    }

    pub fn to_discard_action(&self) -> DraftDiscardActionQueuer {
        DraftDiscardActionQueuer::new(self.metadata_id, Discard::new(self.metadata_id))
    }

    /// Get the message id associated with this draft.
    ///
    /// This function can return `None` if the message has not been created yet.
    pub async fn message_id(&self, tether: &Tether) -> Result<Option<LocalMessageId>, StashError> {
        DraftMetadata::message_id(self.metadata_id, tether).await
    }

    /// Get the conversation id associated with this draft.
    ///
    /// This function can return `None` if the draft is a new empty reply and
    /// the conversation has not yet been created.
    pub async fn conversation_id(
        &self,
        tether: &Tether,
    ) -> Result<Option<LocalConversationId>, StashError> {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, tether).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_conversation_id)
    }

    pub async fn action_undo_send(
        queue: &Queue,
        message_id: LocalMessageId,
    ) -> Result<QueuedActionOutput<UndoSend>, ActionError<UndoSend>> {
        queue.queue_action(UndoSend::new(message_id)).await
    }

    pub async fn load_image(
        metadata_id: MetadataId,
        ctx: &MailUserContext,
        url: &str,
        policy: ImagePolicy,
    ) -> MailContextResult<AttachmentData> {
        ctx.image_loader()
            .load(url, policy, async move |cid| {
                Self::get_embedded_attachment(metadata_id, ctx, cid).await
            })
            .await
            .map_err(Into::into)
    }

    pub async fn get_embedded_attachment(
        metadata_id: MetadataId,
        ctx: &MailUserContext,
        cid: &ContentId,
    ) -> MailContextResult<AttachmentData> {
        let mut tether = ctx.user_stash().connection().await?;

        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(metadata_id, &tether).await?;

        if let Some(attachment) = attachments
            .iter()
            .find(|a| a.content_id.as_ref() == Some(cid))
        {
            let data = attachment.content_data(ctx, &mut tether).await?;

            Ok(AttachmentData {
                data,
                mime: attachment.mime_type.to_string(),
            })
        } else {
            Err(AppError::UnknownCid(cid.clone(), vec![]).into())
        }
    }

    /// Delete an attachment file, but only if it is part of the draft staging area.
    ///
    /// If the removal fails, due to file locks, it will be GCed later by a background task.
    pub async fn delete_attachment_if_in_staging_area(&self, ctx: &MailUserContext, path: &Path) {
        let staging_path = self.attachment_staging_path(ctx);

        if path.starts_with(&staging_path)
            && let Err(e) = fs::remove_file(&staging_path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            // This is a warning as the background process will try again.
            warn!("Failed to delete attachment from staging area at {path:?}: {e:?}");
        }
    }

    /// Add a new `attachment` to this draft.
    ///
    /// Use [`Attachment::create_local`] to create a new attachment first.
    pub async fn add_attachment(
        &mut self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        let upload_action = self.to_add_attachment_action(attachment_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection().await?;
        let result = upload_action
            .queue(
                queue,
                &tether,
                ctx.origin(),
                &mut self.last_draft_save_action_id,
            )
            .await?;

        Ok(result.id)
    }

    pub fn to_add_attachment_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> DraftAttachmentUploadQueuer {
        // create save action before the attachment is registered as we need a message to upload.
        let save_action = self.to_save_action();

        DraftAttachmentUploadQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            attachment_id,
            save_action,
            AttachmentUploadMode::Create,
        )
    }

    pub async fn remove_attachment(
        &self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        let remove_action = self.to_remove_attachment_action(attachment_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection().await?;
        let result = remove_action.queue(queue, &tether, ctx.origin()).await?;

        Ok(result.id)
    }

    pub fn to_remove_attachment_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> DraftAttachmentRemovalQueuer {
        DraftAttachmentRemovalQueuer::new(
            self.metadata_id,
            AttachmentRemovalId::Local(attachment_id),
        )
    }

    pub async fn remove_attachment_with_cid(
        &self,
        ctx: &MailUserContext,
        content_id: ContentId,
    ) -> Result<ActionId, MailContextError> {
        let remove_action = self.to_remove_attachment_action_with_cid(content_id);

        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection().await?;
        let result = remove_action.queue(queue, &tether, ctx.origin()).await?;

        Ok(result.id)
    }

    pub fn to_remove_attachment_action_with_cid(
        &self,
        content_id: ContentId,
    ) -> DraftAttachmentRemovalQueuer {
        DraftAttachmentRemovalQueuer::new(self.metadata_id, AttachmentRemovalId::Cid(content_id))
    }

    pub async fn retry_attachment_operation(
        &mut self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
    ) -> Result<ActionId, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        let metadata = DraftAttachmentMetadata::find_by_id(attachment_id, &tether)
            .await?
            .ok_or_else(|| MailContextError::Other(anyhow!("Attachment metadata not found")))?;

        let queue = ctx.action_queue();

        let action_id = if metadata.is_upload_error() {
            let upload_action = self.to_retry_attachment_upload_action(attachment_id);
            let result = upload_action
                .queue(
                    queue,
                    &tether,
                    ctx.origin(),
                    &mut self.last_draft_save_action_id,
                )
                .await?;
            result.id
        } else if metadata.is_disposition_swap_error() {
            let mut metadata = MetadataBuilder::new()
                .with_resource(&self.metadata_id)
                .expect("This should never fail");

            if let Origin::ShareExt = ctx.origin() {
                metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
            }
            queue
                .queue_action_with_metadata(
                    draft::AttachmentDispositionUpdate::retry(self.metadata_id, attachment_id),
                    metadata.build(),
                )
                .await?
                .id
        } else {
            return Err(MailContextError::Other(anyhow!("Invalid Retry state")));
        };
        Ok(action_id)
    }

    /// Create action queuer where the attachment upload is retried.
    ///
    /// It will only be accepted if the state is [`DraftAttachmentUploadState::Error`]
    pub fn to_retry_attachment_upload_action(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> DraftAttachmentUploadQueuer {
        let save_action = self.to_save_action();

        DraftAttachmentUploadQueuer::new(
            self.metadata_id,
            self.address_id.clone(),
            attachment_id,
            save_action,
            AttachmentUploadMode::Retry,
        )
    }

    pub fn attachment_staging_path(&self, context: &MailUserContext) -> PathBuf {
        draft_attachment_staging_path(context, self.metadata_id)
    }

    pub async fn attachments(&self, tether: &Tether) -> Result<Vec<DraftAttachment>, StashError> {
        DraftAttachment::build_list(self.metadata_id, tether).await
    }

    /// On-the-fly generated head with injected the dark mode styles.
    /// The content of returned string depends on body and modifies it.
    ///
    /// # Parameters
    ///
    /// * `editor_id` - the HTML ID of the editor that wraps the message. The same used to reference DOM in javascript.
    ///
    /// # Modifications to the body
    ///
    /// * If the body contains `!important` flag, it will be removed.
    ///
    /// # Returned HTML
    ///
    /// This function returns HTML that can be inserted INTO `<head>` tag.
    /// It does not provide `<head>` tag on its own.
    /// Therefore, the returned HTML can be inserted alongside with other html nodes.
    ///
    /// ## Example of usage
    ///
    /// ```ignore
    /// let head_to_inject = draft.html_head_content_for_composer(theme_opts, "editor");
    ///
    /// let template = format!("
    /// <html>
    /// <head>
    ///
    ///    <meta ...things set up for the composer />
    ///
    ///    {head_to_inject}
    ///
    /// </head>
    /// <body>
    /// ...
    /// </body>
    /// </html>
    /// ");
    ///
    /// ```
    pub fn html_head_content_for_composer(
        &mut self,
        theme_opts: ThemeOpts,
        editor_id: String,
    ) -> String {
        let color_mode = theme_opts.color_mode();
        let mime_type = self.mime_type();

        let injection = inject_dark_mode(
            mime_type,
            &self.body,
            color_mode,
            BrowserCapabilities {
                supports_dark_mode_via_media_query: theme_opts.supports_dark_mode_via_media_query,
            },
            format!(r#"[id="{editor_id}"]"#),
        );
        self.body = injection.body;

        injection.head
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn body_mut(&mut self) -> &mut String {
        &mut self.body
    }

    pub fn set_body(&mut self, body: String) {
        self.body = body;
    }

    pub async fn attachments_compat(&self, tether: &Tether) -> Result<Vec<Attachment>, StashError> {
        DraftAttachmentMetadata::attachment_for_draft(self.metadata_id, tether).await
    }

    pub fn mime_type(&self) -> MessageMimeType {
        self.mime_type
    }

    pub fn set_mime_type(&mut self, mime_type: MessageMimeType) {
        self.mime_type = mime_type;
    }

    pub fn sanitize_body(&mut self) {
        self.body = maybe_sanitize(self.mime_type(), &self.body);
    }

    pub async fn cancel_schedule_send(
        ctx: &MailUserContext,
        message_id: LocalMessageId,
    ) -> MailContextResult<DateTime<Local>> {
        let mut tether = ctx.user_stash().connection().await?;
        let queue = ctx.action_queue();
        let timeout = Duration::from_secs(15);
        let session = ctx.session();
        let network_monitor_service = ctx.network_monitor_service();
        send::cancel_schedule_send(
            message_id,
            &mut tether,
            queue,
            session,
            timeout,
            network_monitor_service,
        )
        .await
    }

    // Note: this type is currently separate from the draft implementation so that it can be executed
    // in locations where the draft type is not safely shared (e.g.: TUI). A refactor is planned
    // to make this work seamlessly.
    pub fn new_change_sender_address_request(&self) -> DraftAddressChangeRequest {
        DraftAddressChangeRequest::new(
            self.metadata_id,
            self.sender.clone(),
            self.address_id.clone(),
            self.mime_type,
        )
    }

    // Note: this type is currently separate from the draft implementation so that it can be executed
    // in locations where the draft type is not safely shared (e.g.: TUI). A refactor is planned
    // to make this work seamlessly.
    pub fn finalize_sender_address_change_request(&mut self, output: DraftAddressChangeOutput) {
        match output {
            DraftAddressChangeOutput::SenderOnly(sender) => {
                tracing::info!("Updating sender address only to {sender}");
                self.sender = sender;
            }
            DraftAddressChangeOutput::Full(output) => {
                tracing::info!("Updating sender address to {} and signature", output.sender);
                self.address_id = output.address_id;
                self.sender = output.sender;
                // we can only replace the signature if it wasn't empty and the original signature
                // remains intact.
                if self.mime_type == MessageMimeType::TextHtml {
                    let transformer = Transformer::new(self.body());
                    if let Err(e) =
                        transformer.replace_inner_div(PM_SIGNATURE_DIV_CLASS, &output.new_signature)
                    {
                        error!("Error when swapping address signatures: {e}");
                    }
                    self.body = transformer.to_string();
                } else if !output.old_signature.is_empty() {
                    let new_body = self
                        .body
                        .replace(&output.old_signature, &output.new_signature);
                    self.body = new_body;
                }
                self.is_byoe = output.is_byoe
            }
        }
    }

    pub async fn change_sender_address(
        &mut self,
        ctx: &MailUserContext,
        email: String,
    ) -> Result<(), MailContextError> {
        let mut tether = ctx.user_stash().connection().await?;
        let canonical_email = canonicalize_auto(email.as_str());
        let addresses = Address::all_send_enabled(&tether).await?;
        let address = addresses
            .into_iter()
            .find(|v| {
                let this_canonical_email = canonicalize_auto(v.email.as_str());
                this_canonical_email == canonical_email
            })
            .ok_or(SenderAddressChangeError::AddressEmailNotFound(
                email.clone(),
            ))?;
        let address_id =
            address
                .remote_id
                .ok_or(SenderAddressChangeError::AddressHasNoRemoteId(
                    address.local_id.unwrap(),
                ))?;
        self.change_sender_address_impl(ctx, email, address_id, &mut tether)
            .await
    }

    async fn change_sender_address_by_id(
        &mut self,
        ctx: &MailUserContext,
        sender_email: String,
        address_id: AddressId,
    ) -> Result<(), MailContextError> {
        let mut tether = ctx.user_stash().connection().await?;
        self.change_sender_address_impl(ctx, sender_email, address_id, &mut tether)
            .await
    }

    async fn change_sender_address_impl(
        &mut self,
        ctx: &MailUserContext,
        sender_email: String,
        address_id: AddressId,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        if let Some(output) = self
            .new_change_sender_address_request()
            .apply(ctx, sender_email, address_id, tether)
            .await?
        {
            self.finalize_sender_address_change_request(output)
        }

        Ok(())
    }

    pub async fn is_password_protected(&self, tether: &Tether) -> Result<bool, MailContextError> {
        Ok(DraftMetadata::find_by_id(self.metadata_id, tether)
            .await?
            .map(|v| v.password.is_some())
            .unwrap_or(false))
    }

    pub async fn get_password(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Option<EoData>, MailContextError> {
        let encryption_key = ctx.core_context().get_encryption_key()?;
        let tether = ctx.user_stash().connection().await?;

        let metadata = DraftMetadata::find_by_id(self.metadata_id, &tether)
            .await?
            .ok_or(PasswordError::MetadataNotFound(self.metadata_id))?;

        metadata.to_eo_data(&encryption_key)
    }

    pub async fn set_password(
        &self,
        ctx: &MailUserContext,
        password: &str,
        hint: Option<String>,
    ) -> Result<(), MailContextError> {
        Self::set_password_by_id(ctx, self.metadata_id, password, hint).await
    }

    pub async fn set_password_by_id(
        ctx: &MailUserContext,
        metadata_id: MetadataId,
        password: &str,
        hint: Option<String>,
    ) -> Result<(), MailContextError> {
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Err(PasswordError::PasswordTooShort.into());
        }

        let mut tether = ctx.user_stash().connection().await?;

        let mut metadata = DraftMetadata::find_by_id(metadata_id, &tether)
            .await?
            .ok_or(PasswordError::MetadataNotFound(metadata_id))?;

        let session_encryption_key = ctx.core_context().get_encryption_key()?;

        let encrypted_password = EncryptedPassword::new(password, &session_encryption_key)
            .map_err(|_| PasswordError::Encryption)?;

        metadata.password = Some(encrypted_password);
        metadata.password_hint = hint;
        tether.tx(async |tx| metadata.save(tx).await).await?;

        info!("Password protection applied to draft {metadata_id}");

        Ok(())
    }

    pub async fn remove_password(&self, tether: &mut Tether) -> Result<(), MailContextError> {
        Self::remove_password_by_id(tether, self.metadata_id).await
    }

    pub async fn remove_password_by_id(
        tether: &mut Tether,
        metadata_id: MetadataId,
    ) -> Result<(), MailContextError> {
        let mut metadata = DraftMetadata::find_by_id(metadata_id, tether)
            .await?
            .ok_or(PasswordError::MetadataNotFound(metadata_id))?;

        if metadata.password.is_some() {
            metadata.password = None;
            metadata.password_hint = None;
            tether.tx(async |tx| metadata.save(tx).await).await?;

            info!("Password protection removed from draft {metadata_id}");
        }

        Ok(())
    }

    pub async fn set_expiration_time(
        &self,
        tether: &mut Tether,
        expiration_time: DraftExpirationTime,
    ) -> Result<(), MailContextError> {
        Self::set_expiration_time_by_id(tether, self.metadata_id, expiration_time).await
    }

    pub async fn set_expiration_time_by_id(
        tether: &mut Tether,
        metadata_id: MetadataId,
        expiration_time: DraftExpirationTime,
    ) -> Result<(), MailContextError> {
        if let DraftExpirationTime::Custom(expiration_time) = expiration_time {
            let now = UnixTimestamp::now();

            let expiration_time_timestamp = UnixTimestamp::from(expiration_time);
            if expiration_time_timestamp > expiration_time.into() {
                return Err(ExpirationError::ExpirationTimeInThePast.into());
            }

            if expiration_time_timestamp < now.saturating_add(MIN_EXPIRATION_TIME_SECONDS) {
                return Err(ExpirationError::ExpirationTimeLessThan15Min.into());
            }

            let in_28_days =
                ScheduleSendOptions::calculate_next(expiration_time, 28).map_err(|_| {
                    error!("Failed to calculate 30 days into the future");
                    ExpirationError::ExpirationTimeExceeds28Days
                })?;

            if expiration_time > in_28_days {
                return Err(ExpirationError::ExpirationTimeExceeds28Days.into());
            }
        }

        let mut metadata = DraftMetadata::find_by_id(metadata_id, tether)
            .await?
            .ok_or(ExpirationError::MetadataNotFound(metadata_id))?;

        metadata.set_expiration_time(expiration_time);
        tether.tx(async |tx| metadata.save(tx).await).await?;

        if let DraftExpirationTime::Never = expiration_time {
            info!("Expiration removed from draft {metadata_id}");
        } else {
            info!("Expiration applied to draft {metadata_id}");
        }

        Ok(())
    }

    pub async fn expiration_time(
        &self,
        tether: &Tether,
    ) -> Result<DraftExpirationTime, MailContextError> {
        let metadata = DraftMetadata::find_by_id(self.metadata_id, tether)
            .await?
            .ok_or(PasswordError::MetadataNotFound(self.metadata_id))?;

        Ok(metadata.expiration_time())
    }

    pub async fn swap_attachment_disposition_from_inline(
        &self,
        ctx: &MailUserContext,
        content_id: ContentId,
    ) -> Result<(), MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let attachment = Attachment::find_by_content_id(content_id.clone(), &tether)
            .await?
            .ok_or(AttachmentDispositionSwapError::AttachmentNotFoundCid(
                content_id,
            ))?;

        let queue = ctx.action_queue();
        self.swap_attachment_disposition_impl(
            &tether,
            queue,
            ctx.origin(),
            attachment.id(),
            CombinedAttachmentDisposition::Attachment,
        )
        .await
    }
    pub async fn swap_attachment_disposition(
        &self,
        ctx: &MailUserContext,
        attachment_id: LocalAttachmentId,
        new_attachment_disposition: CombinedAttachmentDisposition,
    ) -> Result<(), MailContextError> {
        let queue = ctx.action_queue();
        let tether = ctx.user_stash().connection().await?;
        self.swap_attachment_disposition_impl(
            &tether,
            queue,
            ctx.origin(),
            attachment_id,
            new_attachment_disposition,
        )
        .await
    }

    async fn swap_attachment_disposition_impl(
        &self,
        tether: &Tether,
        queue: &Queue,
        origin: Origin,
        attachment_id: LocalAttachmentId,
        new_attachment_disposition: CombinedAttachmentDisposition,
    ) -> Result<(), MailContextError> {
        let attachment_metadata = DraftAttachmentMetadata::find_by_id(attachment_id, tether)
            .await?
            .ok_or(AttachmentDispositionSwapError::AttachmentNotFound(
                attachment_id,
            ))?;

        let state = attachment_metadata.state();
        if attachment_metadata.deleted
            || matches!(
                state,
                DraftAttachmentUploadState::Error | DraftAttachmentUploadState::Pending
            )
        {
            tracing::error!("Attachment is in invalid state {:?}", state);
            return Err(AttachmentDispositionSwapError::InvalidState(attachment_id).into());
        }

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.metadata_id)
            .expect("Should not fail");

        if let Some(action_id) = attachment_metadata.action_id {
            metadata = metadata.with_dependency(action_id);
        }

        if let Origin::ShareExt = origin {
            metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
        }

        queue
            .queue_action_with_metadata(
                AttachmentDispositionUpdate::new(
                    self.metadata_id,
                    attachment_id,
                    new_attachment_disposition,
                ),
                metadata.build(),
            )
            .await?;
        Ok(())
    }
}

struct DraftSaveActionQueuer {
    id: MetadataId,
    address_id: AddressId,
    action: Save,
    last_save_action_id: Option<ActionId>,
}

impl DraftSaveActionQueuer {
    fn new(
        id: MetadataId,
        address_id: AddressId,
        action: Save,
        last_save_action_id: Option<ActionId>,
    ) -> Self {
        Self {
            id,
            address_id,
            action,
            last_save_action_id,
        }
    }

    #[tracing::instrument(name = "draft::save", skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Save>, MailContextError> {
        // find all attachments that need to be manually queued.
        let pending_attachment_ids =
            DraftAttachmentMetadata::pending_attachments(self.id, tether).await?;

        // We need to be aware of the last save action id to try and replace the existing one.
        // On failure, we only execute after the previous one has finished,
        let last_draft_save_action_id = self.last_save_action_id;

        // If we have attachments that are still uploading we need to schedule a save after that
        // again to update the draft status.
        let mut uploading_attachment_ids =
            DraftAttachmentMetadata::find_attachment_upload_action_ids(self.id, tether).await?;

        let output = queue_or_replace_draft_save(
            queue,
            origin,
            self.action.clone(),
            self.id,
            last_draft_save_action_id,
            [],
            uploading_attachment_ids.clone(),
        )
        .await?;

        // Pending attachments require a draft save first so that we can get a remote id to
        // upload the message.
        if !pending_attachment_ids.is_empty() {
            for attachment_id in pending_attachment_ids {
                let mut metadata = MetadataBuilder::new()
                    .with_resource(&self.id)
                    .expect("This should never fail")
                    .with_dependency(output.id);

                if let Origin::ShareExt = origin {
                    metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
                }

                let output = queue
                    .queue_action_with_metadata(
                        AttachmentUpload::new(
                            self.id,
                            self.address_id.clone(),
                            attachment_id,
                            AttachmentUploadMode::Create,
                        ),
                        metadata.build(),
                    )
                    .await?;

                uploading_attachment_ids.push(output.id);
            }

            // Schedule another save to include the newly scheduled attachments.
            Ok(queue_or_replace_draft_save(
                queue,
                origin,
                self.action,
                self.id,
                Some(output.id),
                [],
                uploading_attachment_ids,
            )
            .await?)
        } else {
            Ok(output)
        }
    }
}

pub struct DraftSendActionQueuer {
    id: MetadataId,
    save_action: DraftSaveActionQueuer,
    send_action: draft::Send,
}

impl DraftSendActionQueuer {
    fn new(id: MetadataId, save_action: DraftSaveActionQueuer, send_action: draft::Send) -> Self {
        Self {
            id,
            save_action,
            send_action,
        }
    }

    #[tracing::instrument(name = "draft::send", skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
        last_draft_save_action_id: &mut Option<ActionId>,
    ) -> Result<QueuedActionOutput<draft::Send>, MailContextError> {
        let save_output = self.save_action.queue(queue, tether, origin).await?;

        *last_draft_save_action_id = Some(save_output.id);

        // We can't send if until all attachments have finished uploading.
        let pending_attachment_ids =
            DraftAttachmentMetadata::find_attachment_upload_action_ids(self.id, tether).await?;

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail")
            .with_dependency(save_output.id)
            .with_dependencies(pending_attachment_ids);

        if let Origin::ShareExt = origin {
            metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
        }

        Ok(queue
            .queue_action_with_metadata(self.send_action, metadata.build())
            .await?)
    }
}

pub struct DraftDiscardActionQueuer {
    id: MetadataId,
    action: Discard,
}

impl DraftDiscardActionQueuer {
    fn new(id: MetadataId, action: Discard) -> Self {
        Self { id, action }
    }

    #[tracing::instrument(name = "draft::discard", skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        origin: Origin,
    ) -> Result<QueuedActionOutput<Discard>, ActionError<Discard>> {
        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail");

        if let Origin::ShareExt = origin {
            metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
        }

        queue
            .queue_action_with_metadata(self.action, metadata.build())
            .await
    }
}

/// Utility type to wrap the queueing of attachments upload.
///
/// We need to make sure that at least one save action is run before this action as we need
/// a remote id to upload.
pub struct DraftAttachmentUploadQueuer {
    id: MetadataId,
    attachment_id: LocalAttachmentId,
    address_id: AddressId,
    save_action: DraftSaveActionQueuer,
    mode: AttachmentUploadMode,
}

impl DraftAttachmentUploadQueuer {
    fn new(
        id: MetadataId,
        address_id: AddressId,
        attachment_id: LocalAttachmentId,
        save_action: DraftSaveActionQueuer,
        mode: AttachmentUploadMode,
    ) -> Self {
        Self {
            id,
            address_id,
            attachment_id,
            save_action,
            mode,
        }
    }

    #[tracing::instrument(name = "draft::attachment_upload", skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
        last_draft_save_action_id: &mut Option<ActionId>,
    ) -> Result<QueuedActionOutput<AttachmentUpload>, MailContextError> {
        let stats =
            DraftAttachmentMetadata::total_attachments_size_and_count(self.id, tether).await?;
        if stats.total_size >= Attachment::MAX_ATTACHMENT_SIZE {
            return Err(AttachmentUploadError::TotalAttachmentSizeTooLarge.into());
        }

        if stats.total >= Attachment::MAX_ATTACHMENTS_PER_MESSAGE {
            return Err(AttachmentUploadError::TooManyAttachments.into());
        }

        // We only need this when creating, if we are retrying this must have already
        // happened.
        if self.mode == AttachmentUploadMode::Create {
            let message_has_remote_id =
                if let Some(local_message_id) = DraftMetadata::message_id(self.id, tether).await? {
                    Message::local_id_counterpart(local_message_id, tether)
                        .await?
                        .is_some()
                } else {
                    false
                };

            // We only want to issue a save action if the draft does not yet have a remote id, otherwise
            // we can't upload the attachment.
            if !message_has_remote_id {
                // If an existing save is ongoing, we want to depend on that action first, otherwise
                // we create a new one ourselves.
                if last_draft_save_action_id.is_none() {
                    *last_draft_save_action_id =
                        Some(self.save_action.queue(queue, tether, origin).await?.id);
                };
            };
        }

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail");

        if let Origin::ShareExt = origin {
            metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
        }

        if let Some(last_draft_save_action_id) = last_draft_save_action_id {
            metadata = metadata.with_dependency(*last_draft_save_action_id)
        }

        // If we are retrying we should wait for the existing one to
        if self.mode == AttachmentUploadMode::Retry {
            let Some(attachment_metadata) =
                DraftAttachmentMetadata::find_by_id(self.attachment_id, tether).await?
            else {
                return Err(
                    AttachmentUploadError::AttachmentDataMissing(self.attachment_id).into(),
                );
            };

            // If the state is not error, we should not allow the retry.
            if attachment_metadata.state() != DraftAttachmentUploadState::Error {
                error!(
                    "Attempting attachment ({}) upload retry on non error state",
                    self.attachment_id
                );
                return Err(AttachmentUploadError::RetryInvalidState(self.attachment_id).into());
            }

            // In case there is still an action, we only want to run after that. Action id is
            // cleaned up on cancel and failure, but due to scheduling it's possible this value
            // is still around.
            if let Some(action_id) = attachment_metadata.action_id {
                metadata = metadata.with_dependency(action_id);
            }
        }

        Ok(queue
            .queue_action_with_metadata(
                AttachmentUpload::new(self.id, self.address_id, self.attachment_id, self.mode),
                metadata.build(),
            )
            .await?)
    }
}

pub(super) enum AttachmentRemovalId {
    Local(LocalAttachmentId),
    Cid(ContentId),
}

pub struct DraftAttachmentRemovalQueuer {
    id: MetadataId,
    attachment_id: AttachmentRemovalId,
}

impl DraftAttachmentRemovalQueuer {
    pub(super) fn new(id: MetadataId, attachment_id: AttachmentRemovalId) -> Self {
        Self { id, attachment_id }
    }

    #[tracing::instrument(name = "draft::attachment_remove", skip_all)]
    pub async fn queue(
        self,
        queue: &Queue,
        tether: &Tether,
        origin: Origin,
    ) -> Result<QueuedActionOutput<AttachmentRemove>, MailContextError> {
        // Find existing attachment metadata.
        let attachment_metadata = match self.attachment_id {
            AttachmentRemovalId::Local(id) => {
                if let Some(attachment_metadata) =
                    DraftAttachmentMetadata::find_by_id(id, tether).await?
                {
                    attachment_metadata
                } else {
                    return Err(AttachmentUploadError::AttachmentMetadataNotFound(id).into());
                }
            }
            AttachmentRemovalId::Cid(id) => {
                if let Some(attachment_metadata) =
                    DraftAttachmentMetadata::find_with_content_id(self.id, id.clone(), tether)
                        .await?
                {
                    attachment_metadata
                } else {
                    return Err(AttachmentUploadError::AttachmentMetadataNotFoundCid(id).into());
                }
            }
        };

        let mut metadata = MetadataBuilder::new()
            .with_resource(&self.id)
            .expect("This should never fail");

        if let Origin::ShareExt = origin {
            metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
        }

        // The removal action can only run when the current action completes.
        if let Some(action_id) = attachment_metadata.action_id {
            // Try to cancel the existing action if it hasn't run yet.
            if let Err(e) = queue.cancel(action_id).await {
                // Only fail if there is a real error
                match e {
                    QueuedError::ActionNotFound(_) | QueuedError::ActionInExecution(_) => {}
                    e => return Err(e.into()),
                }
            }
            metadata = metadata.with_optional_dependency(action_id);
        };

        Ok(queue
            .queue_action_with_metadata(
                AttachmentRemove::new(self.id, attachment_metadata.local_attachment_id),
                metadata.build(),
            )
            .await?)
    }
}

pub fn draft_attachment_staging_path(
    context: &MailUserContext,
    metadata_id: MetadataId,
) -> PathBuf {
    context
        .attachment_staging_path()
        .join(metadata_id.to_string())
}

async fn queue_or_replace_draft_save(
    queue: &Queue,
    origin: Origin,
    save_action: Save,
    metadata_id: MetadataId,
    last_draft_save_action_id: Option<ActionId>,
    other_direct_dependencies: impl IntoIterator<Item = ActionId>,
    other_sequential_dependencies: impl IntoIterator<Item = ActionId>,
) -> Result<QueuedActionOutput<Save>, ActionError<Save>> {
    let mut metadata = MetadataBuilder::new()
        .with_resource(&metadata_id)
        .expect("This should never fail");

    if let Origin::ShareExt = origin {
        metadata = metadata.with_group_override(SHARE_EXT_ACTION_GROUP);
    }

    if let Some(action_id) = last_draft_save_action_id {
        metadata = metadata.with_dependency(action_id);
    }

    let metadata = metadata
        .with_dependencies(other_direct_dependencies)
        .with_optional_dependencies(other_sequential_dependencies)
        .build();

    if let Some(previous_action_id) = last_draft_save_action_id {
        match queue
            .replace_or_queue_action_with_metadata(
                previous_action_id,
                save_action.clone(),
                metadata.clone(),
            )
            .await
        {
            Ok(v) => Ok(v),

            //TODO: More elegant solution
            // It is possible under certain circumstances to issue a replace
            // that can end of up in a cyclic dependency. E.g: Save(A) -> Upload Attachment (B) ->
            // Save (C). Replacing A with C will cause C to Depend on B and B on C rather
            // than A. Extra book keeping is required to prevent this. For now, in the interest
            // of saving time, we just queue the action normally when a cycle is detected.
            Err(ActionError::Queue(proton_action_queue::queue::Error::CyclicDependency)) => {
                queue
                    .queue_action_with_metadata(save_action, metadata)
                    .await
            }

            Err(e) => Err(e),
        }
    } else {
        queue
            .queue_action_with_metadata(save_action, metadata)
            .await
    }
}

// Note: this only exists for the TUI. Will be remove in the draft refactor.
pub struct DraftSenderAddressesDeferred {
    sender_alias: Option<String>,
    address_id: AddressId,
}

impl DraftSenderAddressesDeferred {
    pub async fn run(self, tether: &Tether) -> Result<Vec<Address>, StashError> {
        draft_sender_addresses(self.sender_alias.as_ref(), &self.address_id, tether).await
    }
}
