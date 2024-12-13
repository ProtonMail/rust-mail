use crate::core::datatypes::Id;
use crate::errors::{DraftError, VoidDraftResult};
use crate::mail::datatypes::{AttachmentMetadata, MimeType};
use crate::mail::MailUserSession;
use crate::{async_runtime, uniffi_async};
use itertools::Itertools;
use parking_lot::RwLock;
use proton_action_queue::queue::ActionError;
use proton_mail_common::actions::draft;
use proton_mail_common::datatypes::AttachmentMetadata as RealAttachmentMetadata;
use proton_mail_common::draft::recipients::{
    GroupRecipient, OnBackgroundValidationComplete, Recipient as RealRecipient, RecipientEntry,
    RecipientError, RecipientList, SingleRecipient, ValidatingRecipientList, ValidationState,
};
use proton_mail_common::draft::{Draft as RealDraft, ReplyMode};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::{MailContextError, MailUserContext};
use std::sync::{Arc, Weak};
use tracing::error;

/// Single email recipient.
#[derive(Clone, uniffi::Record)]
pub struct SingleRecipientEntry {
    /// Optional display name component.
    pub name: Option<String>,
    /// Email address component.
    pub email: String,
}

impl From<SingleRecipientEntry> for RecipientEntry {
    fn from(value: SingleRecipientEntry) -> Self {
        Self {
            display_name: value.name,
            email: value.email,
        }
    }
}

/// Errors which occur when adding a single recipient
#[derive(Clone, uniffi::Enum)]
pub enum AddSingleRecipientError {
    /// No errors occurred
    Ok,
    /// The current address already exists in the recipient list.
    Duplicate,
    /// Failed to queue save action for draft.
    SaveFailed,
}

/// Errors which occur when adding a recipients which are part of a group.
#[derive(Clone, uniffi::Enum)]
pub enum AddGroupRecipientError {
    /// No errors occurred
    Ok,
    /// The current addresses already exist in the recipient list.
    Duplicate(Vec<String>),
    /// Failed to queue save action for draft.
    SaveFailed,
}

#[derive(Clone, uniffi::Enum)]
pub enum ComposerRecipient {
    Single(ComposerRecipientSingle),
    Group(ComposerRecipientGroup),
}

impl From<RealRecipient> for ComposerRecipient {
    fn from(value: RealRecipient) -> Self {
        match value {
            RealRecipient::Single(s) => ComposerRecipient::Single(s.into()),
            RealRecipient::Group(g) => ComposerRecipient::Group(g.into()),
        }
    }
}

#[derive(Clone, uniffi::Record)]
pub struct ComposerRecipientSingle {
    pub display_name: Option<String>,
    pub address: String,
    pub valid_state: ComposerRecipientValidState,
}

impl From<SingleRecipient> for ComposerRecipientSingle {
    fn from(value: SingleRecipient) -> Self {
        Self {
            display_name: value.display_name,
            address: value.email,
            valid_state: value.state.into(),
        }
    }
}
#[derive(Clone, uniffi::Record)]
pub struct ComposerRecipientGroup {
    pub display_name: String,
    pub recipients: Vec<ComposerRecipientSingle>,
    pub total_contacts_in_group: u64,
}

impl From<GroupRecipient> for ComposerRecipientGroup {
    fn from(value: GroupRecipient) -> Self {
        Self {
            display_name: value.group_name,
            recipients: value.recipients.into_iter().map_into().collect(),
            total_contacts_in_group: value.total_in_group,
        }
    }
}

/// Validation state of this recipient
#[derive(Clone, uniffi::Enum)]
pub enum ComposerRecipientValidState {
    Valid,
    Invalid(RecipientInvalidReason),
    Validating,
}
#[derive(Clone, uniffi::Enum)]
pub enum RecipientInvalidReason {
    Format,
    DoesNotExist,
    Unknown,
}

impl From<ValidationState> for ComposerRecipientValidState {
    fn from(value: ValidationState) -> Self {
        match value {
            ValidationState::Valid(_) | ValidationState::Unchecked => Self::Valid,
            ValidationState::DoesNotExist => Self::Invalid(RecipientInvalidReason::DoesNotExist),
            ValidationState::InvalidEmail => Self::Invalid(RecipientInvalidReason::Format),
            ValidationState::Validating => Self::Validating,
            ValidationState::Unknown => Self::Invalid(RecipientInvalidReason::Unknown),
        }
    }
}

/// Callback invoked when the recipient list validation triggers an update.
#[uniffi::export(with_foreign)]
pub trait ComposerRecipientValidationCallback: Send + Sync {
    fn on_update(&self);
}

#[derive(Clone)]
struct ComposerRecipientValidationCallbackWrapper(Arc<dyn ComposerRecipientValidationCallback>);

impl OnBackgroundValidationComplete for ComposerRecipientValidationCallbackWrapper {
    async fn recipients_validation_state_updated(&self) {
        let cloned = Arc::clone(&self.0);
        async_runtime().spawn_blocking(move || {
            cloned.on_update();
        });
    }
}

enum ComposerListType {
    To,
    Cc,
    Bcc,
}
#[derive(uniffi::Object)]
pub struct ComposerRecipientList {
    list_type: ComposerListType,
    list: ValidatingRecipientList<ComposerRecipientValidationCallbackWrapper>,
    draft: Weak<Draft>,
    ctx: Arc<MailUserContext>,
}

impl ComposerRecipientList {
    fn new_to_list(
        ctx: Arc<MailUserContext>,
        draft: Weak<Draft>,
        list: RecipientList,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: ComposerListType::To,
            list: ValidatingRecipientList::with_list(list, None),
            draft,
            ctx,
        })
    }
    fn new_bcc_list(
        ctx: Arc<MailUserContext>,
        draft: Weak<Draft>,
        list: RecipientList,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: ComposerListType::Bcc,
            list: ValidatingRecipientList::with_list(list, None),
            draft,
            ctx,
        })
    }

    fn new_cc_list(
        ctx: Arc<MailUserContext>,
        draft: Weak<Draft>,
        list: RecipientList,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: ComposerListType::Cc,
            list: ValidatingRecipientList::with_list(list, None),
            draft,
            ctx,
        })
    }

    async fn save_draft(&self, ctx: &MailUserContext) -> Result<(), ActionError<draft::Save>> {
        let upgrade = self
            .draft
            .upgrade()
            .ok_or(ActionError::Action(MailContextError::Other(
                anyhow::anyhow!("Draft reference no longer valid"),
            )))?;
        let action = {
            let mut draft = upgrade.instance.write();
            match self.list_type {
                ComposerListType::To => draft.to_list = self.list.list(),
                ComposerListType::Cc => draft.cc_list = self.list.list(),
                ComposerListType::Bcc => draft.bcc_list = self.list.list(),
            }
            let action = draft.to_save_action();
            drop(draft);
            action
        };
        ctx.queue().queue_action(action).await?;
        Ok(())
    }
}

#[uniffi::export]
impl ComposerRecipientList {
    /// Set the callback to receive validation updates.
    pub fn set_callback(&self, cb: Arc<dyn ComposerRecipientValidationCallback>) {
        self.list
            .set_callback(Some(ComposerRecipientValidationCallbackWrapper(cb)));
    }
    /// Get the ordered list of recipients.
    pub fn recipients(&self) -> Vec<ComposerRecipient> {
        self.list
            .recipients()
            .into_iter()
            .map(ComposerRecipient::from)
            .collect()
    }

    /// Add a new single recipient to the list.
    pub fn add_single_recipient(&self, recipient: SingleRecipientEntry) -> AddSingleRecipientError {
        // internally the function spawns an async task.
        async_runtime().block_on(async move {
            let email = recipient.email.clone();
            match self
                .list
                .add_single(Arc::clone(&self.ctx), recipient.into())
            {
                Ok(()) => {
                    if let Err(e) = self.save_draft(&self.ctx).await {
                        error!("Failed to queue draft save after recipient add: {e}");
                        self.list.remove_single(&email);
                        AddSingleRecipientError::SaveFailed
                    } else {
                        AddSingleRecipientError::Ok
                    }
                }
                Err(e) => match e {
                    RecipientError::DuplicateAddress(_) => AddSingleRecipientError::Duplicate,
                },
            }
        })
    }

    /// Add or extend the contact group with `group_name` with the given `recipients`.
    ///
    /// Note that `total_contacts_in_group` should be total value of elements in this group. It is
    /// expected that this is retrieved from the contacts api.
    pub fn add_group_recipient(
        &self,
        group_name: &str,
        recipients: Vec<SingleRecipientEntry>,
        total_contacts_in_group: u64,
    ) -> AddGroupRecipientError {
        // internally the function spawns an async task.
        async_runtime().block_on(async move {
            let recipients_cloned = recipients.clone();
            let duplicates = self.list.add_group(
                Arc::clone(&self.ctx),
                group_name,
                recipients.into_iter().map_into(),
                total_contacts_in_group,
            );

            if let Err(e) = self.save_draft(&self.ctx).await {
                error!("Failed to queue draft save after recipient add: {e}");
                self.list.remove_group_recipients(
                    group_name,
                    recipients_cloned.into_iter().map(|e| e.email),
                );
                return AddGroupRecipientError::SaveFailed;
            }

            if duplicates.is_empty() {
                AddGroupRecipientError::Ok
            } else {
                AddGroupRecipientError::Duplicate(duplicates.into_iter().map(|v| v.email).collect())
            }
        })
    }

    /// Remove a single recipient by `email`.
    pub fn remove_single_recipient(&self, email: &str) {
        self.list.remove_single(email);
    }

    /// Remove a contact group by `group_name`
    pub fn remove_group(&self, group_name: &str) {
        self.list.remove_group(group_name);
    }

    /// Remove a recipient with `email` from a contact group with `group_name`.
    pub fn remove_recipient_from_group(&self, group_name: &str, email: &str) {
        self.list.remove_group_recipient(group_name, email);
    }
}

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

/// Represents a draft message which can be crafted as empty or as a reply/forward
/// to an existing message.
#[derive(uniffi::Object)]
pub struct Draft {
    instance: RwLock<RealDraft>,
    ctx: Arc<MailUserContext>,
    to_recipient_list: Arc<ComposerRecipientList>,
    bcc_recipient_list: Arc<ComposerRecipientList>,
    cc_recipient_list: Arc<ComposerRecipientList>,
}
impl Draft {
    fn new_impl(ctx: Arc<MailUserContext>, draft: proton_mail_common::draft::Draft) -> Arc<Self> {
        let to_list = draft.to_list.clone();
        let cc_list = draft.cc_list.clone();
        let bcc_list = draft.bcc_list.clone();
        Arc::new_cyclic(|weak| Self {
            instance: RwLock::new(draft),
            ctx: Arc::clone(&ctx),
            to_recipient_list: ComposerRecipientList::new_to_list(
                Arc::clone(&ctx),
                Weak::clone(weak),
                to_list,
            ),
            bcc_recipient_list: ComposerRecipientList::new_bcc_list(
                Arc::clone(&ctx),
                Weak::clone(weak),
                bcc_list,
            ),
            cc_recipient_list: ComposerRecipientList::new_cc_list(ctx, Weak::clone(weak), cc_list),
        })
    }
}
export_typed_result!(NewDraftResult, Arc<Draft>, DraftError);

/// Create a new draft with the given `create_mode`.
///
/// # Errors
///
/// Return error if action failed.
///
#[uniffi::export]
pub async fn new_draft(session: &MailUserSession, create_mode: DraftCreateMode) -> NewDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        let draft = match create_mode {
            DraftCreateMode::Empty => RealDraft::empty(ctx.user_stash()).await,
            DraftCreateMode::Reply(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::Sender, false).await
            }
            DraftCreateMode::ReplyAll(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::All, false).await
            }
            DraftCreateMode::Forward(id) => {
                RealDraft::reply(&ctx, id.into(), ReplyMode::Forward, false).await
            }
        }
        .map_err(RealProtonMailError::from)?;

        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ctx, draft))
    })
    .await
    .map_err(DraftError::from)
    .into()
}

/// Open an existing draft with `message_id`.
///
/// # Errors
///
/// Returns error if the query failed or the message is not a draft.
///
#[uniffi::export]
pub async fn open_draft(session: &MailUserSession, message_id: Id) -> NewDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        let draft = RealDraft::open(&ctx, message_id.into()).await?;
        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ctx, draft))
    })
    .await
    .map_err(DraftError::from)
    .into()
}

#[uniffi::export]
impl Draft {
    /// Get the sender of the draft.
    pub fn sender(&self) -> String {
        self.instance.read().sender.clone()
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
        self.instance.read().subject.clone()
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        self.instance.read().body.clone()
    }

    /// Set the draft's `subject`.
    pub fn set_subject(&self, subject: String) -> VoidDraftResult {
        let action = {
            let mut draft = self.instance.write();
            draft.subject = subject;
            draft.to_save_action()
        };
        async_runtime()
            .block_on(async {
                save_draft(&self.ctx, action)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftError::from)
            .into()
    }

    /// Set the draft's `body`.
    pub fn set_body(&self, body: String) -> VoidDraftResult {
        let action = {
            let mut draft = self.instance.write();
            draft.body = body;
            draft.to_save_action()
        };
        async_runtime()
            .block_on(async {
                save_draft(&self.ctx, action)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftError::from)
            .into()
    }

    /// Get the draft's attachments
    pub fn attachments(&self) -> Vec<AttachmentMetadata> {
        self.instance
            .read()
            .attachments
            .clone()
            .into_iter()
            .map(|v| RealAttachmentMetadata::from(v).into())
            .collect()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        self.instance.read().mime_type.into()
    }
}

#[uniffi::export]
impl Draft {
    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn save(&self) -> VoidDraftResult {
        let action = {
            let draft = self.instance.read();
            draft.to_save_action()
        };
        let ctx = Arc::clone(&self.ctx);
        uniffi_async(async move {
            ctx.queue()
                .queue_action(action)
                .await
                .map_err(RealProtonMailError::from)?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftError::from)
        .into()
    }

    /// Sends the draft.
    ///
    /// Schedules an action which saves and then sends the draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn send(&self) -> VoidDraftResult {
        let (save_action, send_action) = {
            let draft = self.instance.read();
            (draft.to_save_action(), draft.to_send_action())
        };
        let ctx = Arc::clone(&self.ctx);

        uniffi_async(async move {
            RealDraft::send(ctx.queue(), save_action, send_action?)
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftError::from)
        .into()
    }
}

async fn save_draft(ctx: &MailUserContext, action: draft::Save) -> Result<(), MailContextError> {
    ctx.queue()
        .queue_action(action)
        .await
        .map_err(MailContextError::from)?;
    Ok(())
}
