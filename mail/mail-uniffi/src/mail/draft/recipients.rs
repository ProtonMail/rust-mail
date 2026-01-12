use crate::async_runtime;
use crate::errors::DraftSaveError;
use crate::mail::datatypes::privacy_lock::PrivacyLock;
use crate::mail::draft::CachedDraftData;
use itertools::Itertools;
use non_empty_string::NonEmptyString;
use proton_core_api::services::proton::PrivateString;
use proton_crypto_inbox::lock_icon::UiLock;
use proton_mail_common::MailContextError;
use proton_mail_common::ProtonMailError;
use proton_mail_common::draft::recipients::{
    GroupRecipient, Recipient as RealRecipient, RecipientEntry, RecipientError, SingleRecipient,
    ValidationState,
};
use proton_mail_common::draft::{Draft as RealDraft, Error, RecipientGroupId};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::error;

#[derive(Clone, uniffi::Record)]
pub struct SingleRecipientEntry {
    pub name: Option<String>,
    pub email: String,
}

impl From<SingleRecipientEntry> for RecipientEntry {
    fn from(value: SingleRecipientEntry) -> Self {
        Self {
            name: value.name.map(Into::into),
            email: value.email.into(),
        }
    }
}

#[allow(
    clippy::redundant_closure_for_method_calls,
    reason = "hella awkward otherwise"
)]
impl From<RecipientEntry> for SingleRecipientEntry {
    fn from(value: RecipientEntry) -> Self {
        Self {
            name: value.name.map(|name| name.into_clear_text_string()),
            email: value.email.into_clear_text_string(),
        }
    }
}

/// Errors which occur when adding a single recipient
#[derive(uniffi::Enum)]
pub enum AddSingleRecipientError {
    /// No errors occurred
    Ok,
    /// The current address already exists in the recipient list.
    Duplicate,
    /// Failed to queue save action for draft.
    SaveFailed(DraftSaveError),
    /// Another error occurred
    Other,
}

/// Errors which occur when adding a recipients which are part of a group.
#[derive(uniffi::Enum)]
pub enum AddGroupRecipientError {
    /// No errors occurred
    Ok,
    /// The current addresses already exist in the recipient list.
    Duplicate(Vec<String>),
    /// Failed to queue save action for draft.
    SaveFailed(DraftSaveError),
    /// Empty group name
    EmptyGroupName,
    /// Another error occurred
    Other,
}

/// Errors which occur when removing recipient from the draft
#[derive(uniffi::Enum)]
pub enum RemoveRecipientError {
    /// No errors occurred
    Ok,
    /// Empty group name
    EmptyGroupName,
    /// Failed to queue save action for draft.
    SaveFailed(DraftSaveError),
    /// Another error occurred
    Other,
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
    pub privacy_lock: PrivacyLock,
}

impl From<SingleRecipient> for ComposerRecipientSingle {
    fn from(value: SingleReciient) -> Self {
        Self {
            display_name: value
                .display_name
                .map(PrivateString::into_clear_text_string),
            address: value.email.into_clear_text_string(),
            valid_state: value.state.into(),
            privacy_lock: UiLock::from(value.privacy_lock).into(),
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
            ValidationState::Valid { .. } | ValidationState::Unchecked => Self::Valid,
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
}

impl ComposerRecipientList {
    pub(super) fn new_to_list(draft: RealDraft, state: Arc<RwLock<CachedDraftData>>) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::To,
            state,
            draft,
        })
    }
    pub(super) fn new_bcc_list(draft: RealDraft, state: Arc<RwLock<CachedDraftData>>) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::Bcc,
            state,
            draft,
        })
    }

    pub(super) fn new_cc_list(draft: RealDraft, state: Arc<RwLock<CachedDraftData>>) -> Arc<Self> {
        Arc::new(Self {
            list_type: RecipientGroupId::Cc,
            draft,
            state,
        })
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
        async_runtime().block_on(async move {
            match self
                .draft
                .add_single_recipient(self.list_type, recipient.into())
                .await
            {
                Ok(()) => AddSingleRecipientError::Ok,
                Err(MailContextError::Draft(Error::Recipient(e))) => match e {
                    RecipientError::DuplicateAddress(_) => AddSingleRecipientError::Duplicate,
                },
                Err(MailContextError::Draft(e)) => {
                    error!("Failed to queue draft save after recipient add: {e:?}");
                    let e = ProtonMailError::from(MailContextError::Draft(e));
                    AddSingleRecipientError::SaveFailed(e.into())
                }
                Err(e) => {
                    error!("Failed to add recipient: {e:?}");
                    AddSingleRecipientError::Other
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

        // internally the function spawns an async task.
        async_runtime().block_on(async move {
            match self
                .draft
                .add_recipient_to_group(
                    self.list_type,
                    group_name.clone(),
                    recipients.into_iter().map_into(),
                    total_contacts_in_group,
                )
                .await
            {
                Ok(duplicates) => {
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
                }
                Err(MailContextError::Draft(e)) => {
                    error!("Failed to queue draft save after recipient add: {e:?}");
                    let e = ProtonMailError::from(MailContextError::Draft(e));
                    AddGroupRecipientError::SaveFailed(e.into())
                }
                Err(e) => {
                    error!("Failed to add group recipient {e:?}");
                    AddGroupRecipientError::Other
                }
            }
        })
    }

    /// Remove a single recipient by `email`.
    pub fn remove_single_recipient(&self, email: &str) -> RemoveRecipientError {
        async_runtime().block_on(async move {
            match self
                .draft
                .remove_single_recipient(self.list_type, email.into())
                .await
            {
                Ok(()) => RemoveRecipientError::Ok,
                Err(MailContextError::Draft(e)) => {
                    error!("Failed to queue draft save after recipient remove: {e:?}");
                    let e = ProtonMailError::from(MailContextError::Draft(e));
                    RemoveRecipientError::SaveFailed(e.into())
                }
                Err(e) => {
                    error!("Failed to remove recipient {e:?}");
                    RemoveRecipientError::Other
                }
            }
        })
    }

    /// Remove a contact group by `group_name`
    pub fn remove_group(&self, group_name: String) -> RemoveRecipientError {
        let Ok(group_name) = NonEmptyString::new(group_name) else {
            error!("remove_group with empty group name");
            return RemoveRecipientError::EmptyGroupName;
        };
        async_runtime().block_on(async move {
            match self
                .draft
                .remove_recipient_group(self.list_type, group_name)
                .await
            {
                Ok(()) => RemoveRecipientError::Ok,
                Err(MailContextError::Draft(e)) => {
                    error!("Failed to queue draft save after removing group: {e:?}");
                    let e = ProtonMailError::from(MailContextError::Draft(e));
                    RemoveRecipientError::SaveFailed(e.into())
                }
                Err(e) => {
                    error!("Failed to remove recipient group {e:?}");
                    RemoveRecipientError::Other
                }
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
        async_runtime().block_on(async move {
            match self
                .draft
                .remove_recipient_from_group(self.list_type, email.into(), group_name)
                .await
            {
                Ok(()) => RemoveRecipientError::Ok,
                Err(MailContextError::Draft(e)) => {
                    error!("Failed to queue draft save after removing recipient from group: {e:?}");
                    let e = ProtonMailError::from(MailContextError::Draft(e));
                    RemoveRecipientError::SaveFailed(e.into())
                }
                Err(e) => {
                    error!("Failed to remove recipient from group {e:?}");
                    RemoveRecipientError::Other
                }
            }
        })
    }
}

#[uniffi::export]
pub fn new_recipient(email: &str) -> SingleRecipientEntry {
    RecipientEntry::new(email).into()
}
