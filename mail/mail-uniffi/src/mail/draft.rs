mod attachments;
mod observer;
mod recipients;

use crate::core::datatypes::{Id, UnixTimestamp};
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{
    AttachmentDataResult, DraftCancelScheduleSendError, DraftDiscardError, DraftExpirationError,
    DraftOpenError, DraftPasswordError, DraftSaveError, DraftSendError,
    DraftSenderAddressChangeError, DraftUndoSendError, ProtonError, VoidDraftDiscardResult,
    VoidDraftExpirationResult, VoidDraftPasswordResult, VoidDraftSaveResult, VoidDraftSendResult,
    VoidDraftUndoSendResult,
};
use crate::mail::MailUserSession;
use crate::mail::datatypes::MimeType;
use crate::mail::draft::attachments::AttachmentList;
use crate::mail::draft::observer::DraftSendResult;
use crate::mail::messages::{AttachmentData, ThemeOpts};
use crate::mail::state::MailUserContextPtr;
use crate::{async_runtime, uniffi_async};
use chrono::Local;
use proton_core_api::services::proton::PrivateEmail;
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::draft::recipients::ExpirationFeatureSupportReport;
use proton_mail_common::draft::{
    Draft as RealDraft, DraftExpirationTime as RealDraftExpirationTime,
    DraftSyncStatus as RealDraftSyncStatus, EoData, ReplyMode, ScheduleSendOptions,
    compose::DraftAddressValidationError as RealDraftAddressValidationError,
    compose::DraftAddressValidationResult as RealDraftAddressValidationResult,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::DraftMetadata;
use proton_mail_common::models::DraftSendResult as RealDraftSendResult;
use proton_mail_common::{MailContextError, MailUserContext};
use recipients::ComposerRecipientList;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Draft creation mode.
#[derive(Debug, Copy, Clone, uniffi::Enum)]
pub enum DraftCreateMode {
    /// Empty, new message.
    Empty,
    /// Reply to the sender of a message.
    Reply(Id),
    /// Reply to all recipients of a message and the sender.
    ReplyAll(Id),
    /// Forward the message to
    Forward(Id),
}

#[derive(Debug, uniffi::Record)]
pub struct DraftScheduleSendOptions {
    pub tomorrow_time: UnixTimestamp,
    pub monday_time: UnixTimestamp,
    pub is_custom_option_available: bool,
}

#[derive(Debug, uniffi::Record)]
pub struct HtmlForComposer {
    /// HTML content that should be injected into `<head>` tag.
    ///
    /// It does not provide `<head>` tag on its own.
    /// Therefore, the returned HTML can be inserted alongside with other html nodes.
    pub head_content: String,
    /// Initial body of the draft. Usually contains signature
    /// and replied quote.
    pub initial_body: String,
}
impl From<ScheduleSendOptions<Local>> for DraftScheduleSendOptions {
    fn from(value: ScheduleSendOptions<Local>) -> Self {
        Self {
            tomorrow_time: proton_core_common::datatypes::UnixTimestamp::from(value.time_tomorrow)
                .into(),
            monday_time: proton_core_common::datatypes::UnixTimestamp::from(value.time_next_monday)
                .into(),
            is_custom_option_available: value.is_custom_datetime_available,
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum DraftAddressValidationError {
    SubscriptionRequired,
    Disabled,
    CanNotSend,
    CanNotReceive,
}

impl From<RealDraftAddressValidationError> for DraftAddressValidationError {
    fn from(value: RealDraftAddressValidationError) -> Self {
        match value {
            RealDraftAddressValidationError::SubscriptionRequired => Self::SubscriptionRequired,
            RealDraftAddressValidationError::Disabled => Self::Disabled,
            RealDraftAddressValidationError::CanNotSend => Self::CanNotSend,
            RealDraftAddressValidationError::CanNotReceive => Self::CanNotReceive,
        }
    }
}
#[derive(Debug, uniffi::Record)]
pub struct DraftAddressValidationResult {
    pub email: String,
    pub error: DraftAddressValidationError,
}

impl From<RealDraftAddressValidationResult> for DraftAddressValidationResult {
    fn from(value: RealDraftAddressValidationResult) -> Self {
        Self {
            email: value.email,
            error: value.error.into(),
        }
    }
}

#[derive(Debug, uniffi::Record)]
pub struct DraftPassword {
    pub password: String,
    pub hint: Option<String>,
}

impl From<EoData> for DraftPassword {
    fn from(value: EoData) -> Self {
        Self {
            password: value.password.expose_secret().clone(),
            hint: value.password_hint,
        }
    }
}

struct CachedDraftData {
    subject: String,
    body: String,
    mime_type: MimeType,
    send_result: Option<RealDraftSendResult>,
}
/// Represents a draft message which can be crafted as empty or as a reply/forward
/// to an existing message.
#[derive(uniffi::Object)]
pub struct Draft {
    instance: RealDraft,
    cached: RwLock<CachedDraftData>,
    ctx: MailUserContextPtr,
    to_recipient_list: Arc<ComposerRecipientList>,
    bcc_recipient_list: Arc<ComposerRecipientList>,
    cc_recipient_list: Arc<ComposerRecipientList>,
    attachment_list: Arc<AttachmentList>,
}
impl Draft {
    async fn new_impl(
        ctx: MailUserContextPtr,
        real_ctx: &MailUserContext,
        draft: proton_mail_common::draft::Draft,
    ) -> Result<Arc<Self>, MailContextError> {
        let state = draft.state().await?;
        let staging_path = draft.attachment_staging_path(real_ctx);
        Ok(Arc::new_cyclic(|_| Self {
            cached: RwLock::new(CachedDraftData {
                subject: state.subject,
                body: state.body,
                mime_type: state.mime_type.into(),
                send_result: state.send_result,
            }),
            ctx: ctx.clone(),
            to_recipient_list: ComposerRecipientList::new_to_list(
                ctx.clone(),
                draft.clone(),
                state.to_list,
            ),
            bcc_recipient_list: ComposerRecipientList::new_bcc_list(
                ctx.clone(),
                draft.clone(),
                state.bcc_list,
            ),
            cc_recipient_list: ComposerRecipientList::new_cc_list(
                ctx.clone(),
                draft.clone(),
                state.cc_list,
            ),
            attachment_list: AttachmentList::new(ctx, &staging_path, draft.clone()),
            instance: draft,
        }))
    }
}

/// Represent an open draft.
#[derive(uniffi::Record)]
pub struct OpenDraft {
    /// The draft object itself.
    pub draft: Arc<Draft>,
    /// Whether the draft was synced from the server or we are operating on a cached version.
    pub sync_status: DraftSyncStatus,
}

/// Indicates whether the draft was synced from the server or we are operating on a cached version.
#[derive(uniffi::Enum)]
pub enum DraftSyncStatus {
    /// Draft was not synced from server and we are operating on a cached version
    Cached,
    /// Draft was synced from server.
    Synced,
}

impl From<RealDraftSyncStatus> for DraftSyncStatus {
    fn from(value: RealDraftSyncStatus) -> Self {
        match value {
            RealDraftSyncStatus::Synced => Self::Synced,
            RealDraftSyncStatus::Cached => Self::Cached,
        }
    }
}

#[derive(uniffi::Record)]
pub struct DraftSenderAddressList {
    /// All available addresses which can be used for sending, also includes the
    /// `active` address.
    pub available: Vec<String>,
    /// The current active address.
    pub active: String,
}

/// Create a new draft with the given `create_mode`.
///
/// # Errors
///
/// Return error if action failed.
///
#[uniffi_export]
pub async fn new_draft(
    session: &MailUserSession,
    create_mode: DraftCreateMode,
) -> Result<Arc<Draft>, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();
    uniffi_async(async move {
        let draft = match create_mode {
            DraftCreateMode::Empty => RealDraft::empty(&ctx).await,
            DraftCreateMode::Reply(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::Sender, false, None).await
            }
            DraftCreateMode::ReplyAll(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::All, false, None).await
            }
            DraftCreateMode::Forward(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::Forward, false, None).await
            }
        }
        .map_err(RealProtonMailError::from)?;

        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ptr, &ctx, draft).await?)
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

/// Open an existing draft with `message_id`.
///
/// # Errors
///
/// Returns error if the query failed or the message is not a draft.
///
#[uniffi_export]
pub async fn open_draft(
    session: &MailUserSession,
    message_id: Id,
) -> Result<OpenDraft, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();
    uniffi_async(async move {
        let (draft, status) = RealDraft::open(&ctx, message_id.into()).await?;
        let draft = Draft::new_impl(ptr, &ctx, draft).await?;
        // Revalidate all recipients
        draft.to_recipient_list.check_all_recipients(&ctx);
        draft.cc_recipient_list.check_all_recipients(&ctx);
        draft.bcc_recipient_list.check_all_recipients(&ctx);

        Ok::<_, RealProtonMailError>(OpenDraft {
            draft,
            sync_status: status.into(),
        })
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

#[uniffi_export]
impl Draft {
    /// Get the sender of the draft.
    pub fn sender(&self) -> String {
        //TODO: Improve in follow up with event updates.
        async_runtime().block_on(async { self.instance.sender().await.unwrap_or_default() })
    }

    /// Get the To recipients of the draft.
    pub fn to_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.to_recipient_list)
    }

    /// Get the Cc recipients of the draft.
    pub fn cc_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.cc_recipient_list)
    }

    /// Get the Bcc recipients of the draft.
    pub fn bcc_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.bcc_recipient_list)
    }

    /// Get the draft's subject.
    pub fn subject(&self) -> String {
        async_runtime().block_on(async { self.cached.read().await.subject.clone() })
    }

    /// Returns both HTML content for <head> in the composer editor, as well as modified (as in with dark mode)
    /// body of the draft.
    /// Used to initialize the composer editor.
    ///
    /// **WARNING**: This function modifies the draft content by removing `!important` flag.
    ///
    /// # Parameters
    ///
    /// * `theme_opts` - theme options - used to determine html content theme.
    /// * `editor_id` - the HTML ID of the editor that wraps the message. The same used to reference DOM in javascript.
    ///
    /// # Example of usage
    ///
    /// ```ignore
    /// let html_for_composer = draft.html_for_composer(theme_opts, "editor");
    /// let head_to_inject = html_for_composer.head_content;
    /// let initial_body = html_for_composer.initial_body;
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
    /// {initial_body}
    /// ...
    /// </body>
    /// </html>
    /// ");
    /// ```
    pub fn html_for_composer(
        &self,
        theme_opts: ThemeOpts,
        editor_id: String,
    ) -> Result<HtmlForComposer, ProtonError> {
        let theme_opts = theme_opts.into();
        Ok(async_runtime().block_on(async {
            let instance = self.cached.read().await;
            let head_content = self
                .instance
                .html_head_content_for_composer(theme_opts, editor_id)
                .await?;
            let initial_body = instance.body.clone();

            Ok::<_, RealProtonMailError>(HtmlForComposer {
                head_content,
                initial_body,
            })
        })?)
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        async_runtime().block_on(async { self.cached.read().await.body.clone() })
    }

    /// Set the draft's `subject`.
    #[returns(VoidDraftSaveResult)]
    pub fn set_subject(&self, subject: String) -> Result<(), DraftSaveError> {
        async_runtime()
            .block_on(async {
                let mut instance = self.cached.write().await;
                if instance.subject == subject {
                    return Ok(());
                }
                self.instance.set_subject(subject.clone()).await?;
                instance.subject = subject;
                save_draft(&self.instance)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftSaveError::from)
            .into()
    }

    /// Set the draft's `body`.
    #[returns(VoidDraftSaveResult)]
    pub fn set_body(&self, body: String) -> Result<(), DraftSaveError> {
        async_runtime()
            .block_on(async {
                let mut instance = self.cached.write().await;
                if instance.body == body {
                    return Ok(());
                }
                self.instance.set_body(body.clone()).await?;
                instance.body = body;
                save_draft(&self.instance)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftSaveError::from)
            .into()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        async_runtime().block_on(async { self.cached.read().await.mime_type })
    }

    /// Get the Draft's message id .
    ///
    /// Returns `None` if no message was created.
    pub async fn message_id(self: Arc<Self>) -> Result<Option<Id>, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        uniffi_async::<Option<Id>, RealProtonMailError, _>(async move {
            let metadata_id = self.instance.metadata_id;
            let tether = ctx.user_stash().connection();
            DraftMetadata::message_id(metadata_id, &tether)
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    /// Retrieve the send result associated with draft.
    ///
    /// Note this only loaded with [`open_draft()`].
    pub fn send_result(&self) -> Option<DraftSendResult> {
        async_runtime()
            .block_on(async { self.cached.read().await.send_result.clone().map(Into::into) })
    }

    /// Load an embedded attachment in this draft message.
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    ///
    /// # Errors
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    //NOTE: iOS request we share the same result types between
    // this function and the DecryptedMessageBody equivalent.
    #[returns(AttachmentDataResult)]
    pub async fn get_embedded_attachment(
        self: Arc<Self>,
        cid: String,
    ) -> Result<AttachmentData, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        uniffi_async(async move { self.get_embedded_attachment_impl(&ctx, cid).await })
            .await
            .map_err(ProtonError::from)
            .into()
    }

    /// Same as [`get_embedded_attachment()`], but synchronous.
    //NOTE: iOS request we share the same result types between
    // this function and the DecryptedMessageBody equivalent.
    #[returns(AttachmentDataResult)]
    pub fn get_embedded_attachment_sync(
        self: Arc<Self>,
        cid: String,
    ) -> Result<AttachmentData, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        async_runtime()
            .block_on(self.get_embedded_attachment_impl(&ctx, cid))
            .map_err(ProtonError::from)
            .into()
    }

    /// Get the attachment list.
    pub fn attachment_list(&self) -> Arc<AttachmentList> {
        Arc::clone(&self.attachment_list)
    }

    /// Change the sender address for this draft to the given `email` address.
    pub async fn change_sender_address(
        self: Arc<Self>,
        email: String,
    ) -> Result<(), DraftSenderAddressChangeError> {
        uniffi_async::<_, RealProtonMailError, _>(async move {
            self.instance.change_sender_address(email).await?;
            self.instance
                .save()
                .await
                .map_err(RealProtonMailError::from)?;
            Ok(())
        })
        .await
        .map_err(DraftSenderAddressChangeError::from)
    }

    pub async fn list_sender_addresses(
        self: Arc<Self>,
    ) -> Result<DraftSenderAddressList, ProtonError> {
        Ok(uniffi_async::<_, RealProtonMailError, _>(async move {
            let addresses = self
                .instance
                .sender_addresses()
                .await?
                .into_iter()
                .map(|v| v.email)
                .collect::<Vec<_>>();
            let current = self.instance.sender().await?;
            Ok(DraftSenderAddressList {
                available: addresses,
                active: current,
            })
        })
        .await?)
    }
}

#[uniffi_export]
impl Draft {
    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSaveResult)]
    pub async fn save(self: Arc<Self>) -> Result<(), DraftSaveError> {
        uniffi_async(async move {
            self.instance
                .save()
                .await
                .map_err(RealProtonMailError::from)?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSaveError::from)
        .into()
    }

    /// Sends the draft.
    ///
    /// Schedules an action which saves and then sends the draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSendResult)]
    pub async fn send(self: Arc<Self>) -> Result<(), DraftSendError> {
        uniffi_async(async move {
            self.instance
                .send()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSendError::from)
        .into()
    }

    /// Schedule the sending of the given draft at the `timestamp`.
    #[returns(VoidDraftSendResult)]
    pub async fn schedule(self: Arc<Self>, timestamp: UnixTimestamp) -> Result<(), DraftSendError> {
        let timestamp = proton_core_common::datatypes::UnixTimestamp::from(timestamp)
            .to_date_time()
            .ok_or(DraftSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )))?;
        uniffi_async(async move {
            self.instance
                .schedule_send(timestamp)
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSendError::from)
        .into()
    }

    // Mobile requested this to be sync.
    pub fn schedule_send_options(&self) -> Result<DraftScheduleSendOptions, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        async_runtime()
            .block_on(async {
                RealDraft::schedule_send_options(&ctx)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(ProtonError::from)
            .map(Into::into)
    }

    /// Discard the draft.
    ///
    /// Schedules an action which deletes a draft locally and on the server.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftDiscardResult)]
    pub async fn discard(self: Arc<Self>) -> Result<(), DraftDiscardError> {
        uniffi_async(async move {
            self.instance
                .discard()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftDiscardError::from)
        .into()
    }

    pub fn is_password_protected(&self) -> Result<bool, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .is_password_protected()
                .await
                .map_err(RealProtonMailError::from)
        })?)
    }

    pub fn get_password(&self) -> Result<Option<DraftPassword>, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .get_password()
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })?)
    }

    #[returns(VoidDraftPasswordResult)]
    pub async fn set_password(
        self: Arc<Self>,
        password: String,
        hint: Option<String>,
    ) -> Result<(), DraftPasswordError> {
        let password = SecretString::new(password);
        uniffi_async(async move {
            self.instance
                .set_password_with_secret(password, hint)
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftPasswordError::from)
        .into()
    }

    #[returns(VoidDraftPasswordResult)]
    pub async fn remove_password(self: Arc<Self>) -> Result<(), DraftPasswordError> {
        uniffi_async(async move {
            self.instance
                .remove_password()
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftPasswordError::from)
        .into()
    }

    #[returns(VoidDraftExpirationResult)]
    pub async fn set_expiration_time(
        self: Arc<Self>,
        expiration_time: DraftExpirationTime,
    ) -> Result<(), DraftExpirationError> {
        let expiration_time = RealDraftExpirationTime::try_from(expiration_time)?;
        uniffi_async(async move {
            self.instance
                .set_expiration_time(expiration_time.into())
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftExpirationError::from)
        .into()
    }

    pub fn expiration_time(&self) -> Result<DraftExpirationTime, ProtonError> {
        Ok(async_runtime().block_on(async move {
            self.instance
                .expiration_time()
                .await
                .map(Into::into)
                .map_err(RealProtonMailError::from)
        })?)
    }

    pub fn address_validation_result(&self) -> Option<DraftAddressValidationResult> {
        async_runtime().block_on(async move {
            self.instance
                .address_validation_result()
                .await
                .unwrap_or(None)
                .map(Into::into)
        })
    }

    pub fn clear_address_validation_error(&self) {
        async_runtime().block_on(async move {
            if let Err(e) = self.instance.clear_address_validation_result().await {
                tracing::error!("failed to clear address validation error: {:?}", e);
            }
        });
    }

    pub fn validate_recipients_expiration_feature(
        &self,
    ) -> Result<DraftRecipientExpirationFeatureReport, ProtonError> {
        let Some(_) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        async_runtime().block_on(async move {
            if let Ok(is_password_protected) = self.instance.is_password_protected().await
                && is_password_protected
            {
                return Ok(DraftRecipientExpirationFeatureReport::default());
            }
            let mut report = ExpirationFeatureSupportReport::default();
            self.to_recipient_list
                .validate_expiration_feature(&mut report);
            self.cc_recipient_list
                .validate_expiration_feature(&mut report);
            self.bcc_recipient_list
                .validate_expiration_feature(&mut report);
            Ok(report.into())
        })
    }
}

impl Draft {
    async fn get_embedded_attachment_impl(
        &self,
        _: &MailUserContext,
        cid: String,
    ) -> Result<AttachmentData, RealProtonMailError> {
        let att = self
            .instance
            .get_embedded_attachment(&ContentId::from(cid))
            .await
            .map_err(RealProtonMailError::from)?;
        Ok::<_, RealProtonMailError>(AttachmentData {
            data: att.data,
            mime: att.mime,
        })
    }
}

/// Cancel the sending of message with `message_id`.
///
/// Note that will only work if the message has been sent with a send delay.
#[uniffi_export]
#[returns(VoidDraftUndoSendResult)]
pub async fn draft_undo_send(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftUndoSendError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        RealDraft::action_undo_send(ctx.action_queue(), message_id.into()).await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftUndoSendError::from)
    .into()
}

/// Discard a Draft by with the given `message_id`.
///
/// Note that this requires that the given message interacted with any of the [`Draft`] APIs
/// in the past.
#[uniffi_export]
#[returns(VoidDraftDiscardResult)]
pub async fn draft_discard(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftDiscardError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection();
        RealDraft::action_discard(message_id.into(), &tether, ctx.action_queue(), ctx.origin())
            .await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftDiscardError::from)
    .into()
}

#[derive(uniffi::Record)]
pub struct DraftCancelScheduledSendInfo {
    pub last_scheduled_time: UnixTimestamp,
}

/// Cancel the scheduled send of message with `message_id`.
///
/// Note that will only work if the message has been scheduled for sending.
#[uniffi_export]
pub async fn draft_cancel_schedule_send(
    session: &MailUserSession,
    message_id: Id,
) -> Result<DraftCancelScheduledSendInfo, DraftCancelScheduleSendError> {
    let ctx = session.ctx()?;
    let old_time = uniffi_async(async move {
        let old_time = RealDraft::cancel_schedule_send(&ctx, message_id.into()).await?;
        Ok::<_, RealProtonMailError>(old_time)
    })
    .await
    .map_err(DraftCancelScheduleSendError::from)?;

    Ok(DraftCancelScheduledSendInfo {
        last_scheduled_time: proton_core_common::datatypes::UnixTimestamp::from(old_time).into(),
    })
}

async fn save_draft(draft: &RealDraft) -> Result<(), MailContextError> {
    draft.save().await?;
    Ok(())
}

#[derive(Debug, uniffi::Enum)]
pub enum DraftExpirationTime {
    Never,
    OneHour,
    OneDay,
    ThreeDays,
    Custom(UnixTimestamp),
}

impl From<RealDraftExpirationTime> for DraftExpirationTime {
    fn from(value: RealDraftExpirationTime) -> Self {
        match value {
            RealDraftExpirationTime::Never => DraftExpirationTime::Never,
            RealDraftExpirationTime::OneHour => DraftExpirationTime::OneHour,
            RealDraftExpirationTime::OneDay => DraftExpirationTime::OneDay,
            RealDraftExpirationTime::ThreeDays => DraftExpirationTime::ThreeDays,
            RealDraftExpirationTime::Custom(dt) => DraftExpirationTime::Custom(dt.into()),
        }
    }
}

impl TryFrom<DraftExpirationTime> for RealDraftExpirationTime {
    type Error = DraftExpirationError;

    fn try_from(value: DraftExpirationTime) -> Result<Self, Self::Error> {
        match value {
            DraftExpirationTime::Never => Ok(RealDraftExpirationTime::Never),
            DraftExpirationTime::OneHour => Ok(RealDraftExpirationTime::OneHour),
            DraftExpirationTime::OneDay => Ok(RealDraftExpirationTime::OneDay),
            DraftExpirationTime::ThreeDays => Ok(RealDraftExpirationTime::ThreeDays),
            DraftExpirationTime::Custom(timestamp) => {
                let expiration_time = proton_core_common::datatypes::UnixTimestamp::from(timestamp)
                    .to_date_time()
                    .ok_or(DraftExpirationError::Other(ProtonError::Unexpected(
                        UnexpectedError::Internal,
                    )))?;
                Ok(RealDraftExpirationTime::Custom(expiration_time))
            }
        }
    }
}

#[derive(Default, Debug, uniffi::Record)]
pub struct DraftRecipientExpirationFeatureReport {
    pub unsupported: Vec<String>,
    pub unknown: Vec<String>,
}

impl From<ExpirationFeatureSupportReport> for DraftRecipientExpirationFeatureReport {
    fn from(value: ExpirationFeatureSupportReport) -> Self {
        Self {
            unsupported: value
                .unsupported
                .into_iter()
                .map(PrivateEmail::into_clear_text_string)
                .collect(),
            unknown: value
                .unknown
                .into_iter()
                .map(PrivateEmail::into_clear_text_string)
                .collect(),
        }
    }
}
