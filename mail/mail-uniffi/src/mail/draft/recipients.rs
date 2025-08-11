use crate::async_runtime;
use crate::mail::draft::CachedDraftData;
use crate::mail::state::MailUserContextPtr;
use itertools::Itertools;
use non_empty_string::NonEmptyString;
use proton_core_api::services::proton::{PrivateEmail, PrivateString};
use proton_mail_common::draft::recipients::{
    GroupRecipient, Recipient as RealRecipient, RecipientEntry, RecipientError, SingleRecipient,
    ValidationState,
};
use proton_mail_common::draft::{Draft as RealDraft, Error, RecipientGroupId};
use proton_mail_common::{MailContextError, MailUserContext};
use std::sync::Arc;
use tokio::sync::RwLock;
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
            display_name: value.name.map(Into::into),
            email: value.email.into(),
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
    /// Empty group name
    EmptyGroupName,
}

/// Errors which occur when removing recipient from the draft
#[derive(Clone, uniffi::Enum)]
pub enum RemoveRecipientError {
    /// No errors occurred
    Ok,
    /// Empty group name
    EmptyGroupName,
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
            display_name: value
                .display_name
                .map(PrivateString::into_clear_text_string),
            address: value.email.into_clear_text_string(),
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
            display_name: value.group_name.into_inner(),
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

#[derive(uniffi::Object)]
pub struct ComposerRecipientList {
    list_type: RecipientGroupId,
    state: Arc<RwLock<CachedDraftData>>,
    draft: RealDraft,
    ctx: MailUserContextPtr,
}

impl ComposerRecipientList {
    pub(super) fn new_to_list(
        ctx: MailUserContextPtr,
        draft: RealDraft,
        state: Arc<RwLock<CachedDraftData>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::To,
            state,
            draft,
            ctx,
        })
    }
    pub(super) fn new_bcc_list(
        ctx: MailUserContextPtr,
        draft: RealDraft,
        state: Arc<RwLock<CachedDraftData>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::Bcc,
            state,
            draft,
            ctx,
        })
    }

    pub(super) fn new_cc_list(
        ctx: MailUserContextPtr,
        draft: RealDraft,
        state: Arc<RwLock<CachedDraftData>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::Cc,
            draft,
            state,
            ctx,
        })
    }

    async fn save_draft(&self, _: &MailUserContext) -> Result<(), MailContextError> {
        self.draft.save().await?;
        Ok(())
    }
}

#[uniffi_export]
impl ComposerRecipientList {
    /// Set the callback to receive validation updates.
    pub fn set_callback(&self, cb: Arc<dyn ComposerRecipientValidationCallback>) {
        async_runtime().block_on(async {
            let mut state = self.state.write().await;
            match self.list_type {
                RecipientGroupId::To => {
                    state.to_list_cb = Some(cb);
                }
                RecipientGroupId::Cc => {
                    state.cc_list_cb = Some(cb);
                }
                RecipientGroupId::Bcc => {
                    state.bcc_list_cb = Some(cb);
                }
            }
        });
    }
    /// Get the ordered list of recipients.
    pub fn recipients(&self) -> Vec<ComposerRecipient> {
        //TODO: change this after the clients change their logic to get updates via the callback
        // rather than right after they modify the list
        async_runtime().block_on(async {
            self.draft
                .recipients(self.list_type)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect()
        })
        /*
        async_runtime().block_on(async {
            let state = self.state.read().await;
            match self.list_type {
                RecipientGroupId::To => state.to_list.clone(),
                RecipientGroupId::Cc => state.cc_list.clone(),
                RecipientGroupId::Bcc => state.bcc_list.clone(),
            }
        })*/
    }

    /// Add a new single recipient to the list.
    pub fn add_single_recipient(&self, recipient: SingleRecipientEntry) -> AddSingleRecipientError {
        let Some(ctx) = self.ctx.upgrade() else {
            return AddSingleRecipientError::SaveFailed;
        };
        // internally the function spawns an async task.
        async_runtime().block_on(async move {
            let email = recipient.email.clone();
            match self
                .draft
                .add_single_recipient(self.list_type, recipient.into())
                .await
            {
                Ok(()) => {
                    //TODO: Auto save logic
                    if let Err(e) = self.save_draft(&ctx).await {
                        error!("Failed to queue draft save after recipient add: {e:?}");
                        let _ = self
                            .draft
                            .remove_single_recipient(self.list_type, email.into())
                            .await;
                        AddSingleRecipientError::SaveFailed
                    } else {
                        AddSingleRecipientError::Ok
                    }
                }
                Err(MailContextError::Draft(Error::Recipient(e))) => match e {
                    RecipientError::DuplicateAddress(_) => AddSingleRecipientError::Duplicate,
                },
                Err(e) => {
                    error!("Unknown error occurred: {e:?}");
                    AddSingleRecipientError::SaveFailed
                }
            }
        })
    }

    /// Add or extend the contact group with `group_name` with the given `recipients`.
    ///
    /// Note that `total_contacts_in_group` should be total value of elements in this group. It is
    /// expected that this is retrieved from the contacts api.
    pub fn add_group_recipient(
        &self,
        group_name: String,
        recipients: Vec<SingleRecipientEntry>,
        total_contacts_in_group: u64,
    ) -> AddGroupRecipientError {
        let Ok(group_name) = NonEmptyString::new(group_name) else {
            return AddGroupRecipientError::EmptyGroupName;
        };

        let Some(ctx) = self.ctx.upgrade() else {
            return AddGroupRecipientError::SaveFailed;
        };
        // internally the function spawns an async task.
        async_runtime().block_on(async move {
            let recipients_cloned = recipients.clone();
            let Ok(duplicates) = self
                .draft
                .add_recipient_to_group(
                    self.list_type,
                    group_name.clone(),
                    recipients.into_iter().map_into(),
                    total_contacts_in_group,
                )
                .await
            else {
                return AddGroupRecipientError::SaveFailed;
            };

            //TODO: Auto save logic
            if let Err(e) = self.save_draft(&ctx).await {
                error!("Failed to queue draft save after recipient add: {e:?}");
                let _ = self
                    .draft
                    .remove_recipients_from_group(
                        self.list_type,
                        recipients_cloned
                            .into_iter()
                            .map(|e| PrivateEmail::new(e.email)),
                        group_name,
                    )
                    .await;
                return AddGroupRecipientError::SaveFailed;
            }

            if duplicates.is_empty() {
                AddGroupRecipientError::Ok
            } else {
                AddGroupRecipientError::Duplicate(
                    duplicates
                        .into_iter()
                        .map(|v| v.email.into_clear_text_string())
                        .collect(),
                )
            }
        })
    }

    /// Remove a single recipient by `email`.
    pub fn remove_single_recipient(&self, email: &str) -> RemoveRecipientError {
        let Some(ctx) = self.ctx.upgrade() else {
            return RemoveRecipientError::SaveFailed;
        };
        async_runtime().block_on(async move {
            //TODO: draft auto save
            let _ = self
                .draft
                .remove_single_recipient(self.list_type, email.into())
                .await;
            if let Err(e) = self.save_draft(&ctx).await {
                error!("Failed to queue draft save after recipient remove: {e:?}");
                RemoveRecipientError::SaveFailed
            } else {
                RemoveRecipientError::Ok
            }
        })
    }

    /// Remove a contact group by `group_name`
    pub fn remove_group(&self, group_name: String) -> RemoveRecipientError {
        let Ok(group_name) = NonEmptyString::new(group_name) else {
            error!("remove_group with empty group name");
            return RemoveRecipientError::EmptyGroupName;
        };
        let Some(ctx) = self.ctx.upgrade() else {
            return RemoveRecipientError::SaveFailed;
        };
        async_runtime().block_on(async move {
            //TODO: draft auto save
            let _ = self
                .draft
                .remove_recipient_group(self.list_type, group_name)
                .await;
            if let Err(e) = self.save_draft(&ctx).await {
                error!("Failed to queue draft save after removing group: {e:?}");
                RemoveRecipientError::SaveFailed
            } else {
                RemoveRecipientError::Ok
            }
        })
    }

    /// Remove a recipient with `email` from a contact group with `group_name`.
    pub fn remove_recipient_from_group(
        &self,
        group_name: String,
        email: &str,
    ) -> RemoveRecipientError {
        let Ok(group_name) = NonEmptyString::new(group_name) else {
            error!("remove_recipient_from_group with empty group name");
            return RemoveRecipientError::EmptyGroupName;
        };
        let Some(ctx) = self.ctx.upgrade() else {
            return RemoveRecipientError::SaveFailed;
        };
        async_runtime().block_on(async move {
            //TODO: draft auto save
            let _ = self
                .draft
                .remove_recipient_from_group(self.list_type, email.into(), group_name)
                .await;
            if let Err(e) = self.save_draft(&ctx).await {
                error!("Failed to queue draft save after removing recipient from group: {e:?}");
                RemoveRecipientError::SaveFailed
            } else {
                RemoveRecipientError::Ok
            }
        })
    }
}
