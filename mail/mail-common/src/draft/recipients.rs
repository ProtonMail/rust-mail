use crate::MailUserContext;
use crate::datatypes::MessageRecipient;
use crate::models::MessageReplyTo;
use non_empty_string::NonEmptyString;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{
    GetKeysAllOptions, PrivateEmail, PrivateEmailRef, PrivateString,
};
use proton_core_api::{consts::CoreBundle, services::proton::ProtonCore};
use proton_core_common::models::ContactEmail;
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::error;

#[cfg(test)]
#[path = "../tests/draft/recipients.rs"]
mod tests;

/// Newtype where the Some(String) is never empty.
// That statement is not true as one can always mutate the string to make it empty but don't tell anybody.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct MaybeEmptyString(pub Option<NonEmptyString>);

impl MaybeEmptyString {
    pub fn from_option(value: Option<String>) -> Self {
        value.unwrap_or_default().into()
    }

    pub fn into_option(self) -> Option<String> {
        self.0.map(NonEmptyString::into_inner)
    }

    /// Actually gets an empty string if the string is empty.
    pub fn into_string(self) -> String {
        self.0.map(NonEmptyString::into_inner).unwrap_or_default()
    }
}

impl From<String> for MaybeEmptyString {
    fn from(value: String) -> Self {
        Self(NonEmptyString::try_from(value).ok())
    }
}

/// State of the recipient validation
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ValidationState {
    /// Has been checked by the proton server. If true, it means it is a
    /// proton address.
    Valid(bool),
    /// This proton address does not exist
    DoesNotExist,
    /// The email is formatted correctly
    InvalidEmail,
    /// This recipient has not yet been checked, there may be no network
    /// or the validation hasn't started.
    Unchecked,
    /// This recipient being validated.
    Validating,
    /// This triggers when there is an error during validation that
    /// was not accounted for.
    Unknown,
}

impl From<ApiServiceError> for ValidationState {
    fn from(value: ApiServiceError) -> Self {
        Self::from(&value)
    }
}

impl From<&ApiServiceError> for ValidationState {
    fn from(value: &ApiServiceError) -> Self {
        if value.is_network_failure() {
            return ValidationState::Unchecked;
        }

        if let Some(proton_error) = value.to_proton_error() {
            if proton_error.code == CoreBundle::KeyGetInputInvalid as u32 {
                // 33101 = Invalid email address
                return ValidationState::InvalidEmail;
            } else if proton_error.code == CoreBundle::KeyGetAddressMissing as u32 {
                // 33102 = Proton Address does not exist
                return ValidationState::DoesNotExist;
            }
        }

        ValidationState::Unknown
    }
}

/// Represents a single recipient
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SingleRecipient {
    /// Optional display name for the recipient.
    pub display_name: Option<PrivateString>,
    /// Recipient's email
    pub email: PrivateEmail,
    /// Validation state.
    pub state: ValidationState,
}

/// Represents list of recipients in named group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupRecipient {
    /// Recipients that compose this group
    pub recipients: Vec<SingleRecipient>,
    /// Name of the group
    pub group_name: NonEmptyString,
    /// Total number of addresses in this group.
    pub total_in_group: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientError {
    #[error("Address {0} already exists in the recipient list")]
    DuplicateAddress(PrivateEmail),
}

/// An email recipient.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum Recipient {
    Single(SingleRecipient),
    Group(GroupRecipient),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecipientEntry {
    pub display_name: Option<PrivateString>,
    pub email: PrivateEmail,
}

/// Abstraction over possible contact group resolvers.
pub trait ContactGroupResolver {
    /// Resolve the total number of members in a contact group.
    ///
    /// Return `None` on error or if the group can't be found.
    fn resolve_contact_group_total(
        &self,
        name: &NonEmptyString,
    ) -> impl Future<Output = Option<u64>>;
}

/// Default contact group resolver, always returns `None`.
#[derive(Default, Copy, Clone)]
pub struct NullContactGroupResolver;
impl ContactGroupResolver for NullContactGroupResolver {
    async fn resolve_contact_group_total(&self, _: &NonEmptyString) -> Option<u64> {
        None
    }
}

pub struct ProtonContactGroupResolver<'t>(&'t Tether);

impl ContactGroupResolver for ProtonContactGroupResolver<'_> {
    async fn resolve_contact_group_total(&self, group_name: &NonEmptyString) -> Option<u64> {
        ContactEmail::count_in_contact_group_by_name(group_name.clone().into_inner(), self.0)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to load contact group: {e:?}");
                None
            })
            .map(|v| v as u64)
    }
}

impl<'t> ProtonContactGroupResolver<'t> {
    pub fn new(tether: &'t Tether) -> Self {
        Self(tether)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExpirationFeatureSupportReport {
    pub supported: HashSet<PrivateEmail>,
    pub unsupported: HashSet<PrivateEmail>,
    pub unknown: HashSet<PrivateEmail>,
}

impl ExpirationFeatureSupportReport {
    fn check(&mut self, email: PrivateEmailRef, validation_state: ValidationState) {
        match validation_state {
            ValidationState::Valid(true) => {
                self.supported.insert(email.to_owned());
            }
            ValidationState::Valid(false) => {
                // API is currently returning `IsProton:0` for known official proton email addresses,
                // so we have to manually match here to correctly detect this.
                for domain in PROTON_EMAIL_DOMAINS {
                    if email.as_clear_text_str().to_lowercase().ends_with(domain) {
                        self.supported.insert(email.to_owned());
                        return;
                    }
                }
                self.unsupported.insert(email.to_owned());
            }
            _ => {
                // If we are unable to validate at the moment, we can still
                // quickly validate if some address ends in a known domain as they are
                // always supported.
                for domain in PROTON_EMAIL_DOMAINS {
                    if email.as_clear_text_str().to_lowercase().ends_with(domain) {
                        self.supported.insert(email.to_owned());
                        return;
                    }
                }
                self.unknown.insert(email.to_owned());
            }
        };
    }

    fn add_as_supported(&mut self, email: PrivateEmailRef, validation_state: ValidationState) {
        match validation_state {
            ValidationState::Valid(_)
            | ValidationState::Unchecked
            | ValidationState::Validating => {
                self.supported.insert(email.to_owned());
            }
            _ => {}
        }
    }
}

/// A list of email recipients.
///
/// This recipient list is meant to be used in conjunction with the
/// contact picker. Contacts are resolved by the contact APIs and then
/// fed to this list.
///
/// Unless the email format is not valid, all recipients are added in an
/// unchecked state. Before the Draft is sent we will verify that the
/// recipients are valid. If the recipient is a proton address, we will
/// also check whether the address actually exists.
///
#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct RecipientList {
    recipients: Vec<Recipient>,
}

impl RecipientList {
    /// Create a new empty list.
    pub fn new() -> Self {
        Self {
            recipients: Vec::new(),
        }
    }

    /// Create a list from a [`Message`]'s recipient list.
    ///
    /// This function expect the data to be valid. Errors are silently
    /// ignored.
    pub async fn from_message_recipients(
        contact_group_resolver: &impl ContactGroupResolver,
        recipients: impl IntoIterator<Item = MessageRecipient>,
    ) -> Self {
        let mut list = Self::new();
        for recipient in recipients {
            let entry = RecipientEntry {
                email: recipient.address,
                display_name: if recipient.name.is_empty() {
                    None
                } else {
                    Some(recipient.name)
                },
            };
            if let Some(name) = recipient.group.0 {
                //if group is not found, assume total is the number of entries
                //in the current group.
                list.add_group(name, [entry], 0);
            } else if let Err(e) = list.add_single(entry) {
                error!("Failed to add single recipient: {e:?}");
            }
        }

        // path all groups that have 0 length
        for recipient in &mut list.recipients {
            if let Recipient::Group(group) = recipient {
                group.total_in_group = contact_group_resolver
                    .resolve_contact_group_total(&group.group_name)
                    .await
                    .unwrap_or(group.recipients.len() as u64)
            }
        }

        list
    }

    pub fn from_message_reply_to(reply_tos: impl IntoIterator<Item = MessageReplyTo>) -> Self {
        let mut list = Self::new();
        for recipient in reply_tos {
            let entry = RecipientEntry {
                email: recipient.address,
                display_name: if recipient.name.is_empty() {
                    None
                } else {
                    Some(recipient.name)
                },
            };
            if let Err(e) = list.add_single(entry) {
                error!("Failed to add single recipient: {e:?}");
            }
        }
        list
    }

    /// Add a new recipient to the list.
    ///
    /// # Errors
    ///
    /// Returns error if the address is not valid or was already added
    /// to this list.
    pub fn add_single(
        &mut self,
        entry: RecipientEntry,
    ) -> Result<&mut SingleRecipient, RecipientError> {
        self.add_single_with_state(entry, ValidationState::Unchecked)
    }

    fn add_single_with_state(
        &mut self,
        entry: RecipientEntry,
        state: ValidationState,
    ) -> Result<&mut SingleRecipient, RecipientError> {
        if self.is_duplicate_address(entry.email.as_ref()) {
            return Err(RecipientError::DuplicateAddress(entry.email));
        }

        let state = if proton_core_common::validation::is_valid_email_address(&entry.email) {
            state
        } else {
            ValidationState::InvalidEmail
        };

        self.recipients.push(Recipient::Single(SingleRecipient {
            display_name: entry.display_name,
            email: entry.email,
            state,
        }));
        match self
            .recipients
            .last_mut()
            .expect("always has a single recipient")
        {
            Recipient::Single(single) => Ok(single),
            Recipient::Group(_) => unreachable!(),
        }
    }

    /// Remove a recipient from this list by `email`.
    pub fn remove_single(&mut self, email: &str) {
        self.recipients.retain(|r| {
            let Recipient::Single(recipient) = r else {
                return true;
            };

            recipient.email.as_clear_text_str() != email
        });
    }

    /// Add a new recipient group to this list.
    ///
    /// If the group does not exist, it will be created.
    ///
    /// If the group already exists, the recipients will be added to this group.
    ///
    /// If duplicates are found, they are returned by this function.
    ///
    /// The `total_in_group` should always match the total number of members
    /// of the contact group. The recipient list group should only contain
    /// active members of that group.
    pub fn add_group(
        &mut self,
        group_name: NonEmptyString,
        entries: impl IntoIterator<Item = RecipientEntry>,
        total_in_group: u64,
    ) -> (&mut GroupRecipient, Vec<RecipientEntry>) {
        self.add_group_with_state(
            group_name,
            entries,
            total_in_group,
            ValidationState::Unchecked,
        )
    }

    fn add_group_with_state(
        &mut self,
        group_name: NonEmptyString,
        entries: impl IntoIterator<Item = RecipientEntry>,
        total_in_group: u64,
        state: ValidationState,
    ) -> (&mut GroupRecipient, Vec<RecipientEntry>) {
        let mut duplicates = Vec::new();
        let iter = entries.into_iter();
        let mut recipients = Vec::with_capacity(iter.size_hint().0);

        for recipient in iter {
            if self.is_duplicate_address(recipient.email.as_ref()) {
                duplicates.push(recipient);
                continue;
            }

            let state = if proton_core_common::validation::is_valid_email_address(&recipient.email)
            {
                state
            } else {
                ValidationState::InvalidEmail
            };

            recipients.push(SingleRecipient {
                display_name: recipient.display_name,
                email: recipient.email,
                state,
            });
        }

        let group = self.get_or_create_group(group_name);
        group.total_in_group = total_in_group;
        group.recipients.extend(recipients);
        (group, duplicates)
    }

    /// Remove an entire group from the recipient list.
    pub fn remove_group(&mut self, group_name: &NonEmptyString) {
        self.recipients.retain(|r| {
            let Recipient::Group(recipient) = r else {
                return true;
            };

            group_name != &recipient.group_name
        })
    }

    /// Remove a recipient with `email` from the group with `group_name`.
    pub fn remove_group_recipient(&mut self, group_name: &NonEmptyString, email: &str) {
        self.remove_group_recipients(group_name, std::iter::once(email));
    }

    /// Remove recipients with `emails` from the group with `group_name`.
    pub fn remove_group_recipients<T: AsRef<str>>(
        &mut self,
        group_name: &NonEmptyString,
        emails: impl IntoIterator<Item = T>,
    ) {
        if let Some(group) = self.find_group_mut(group_name) {
            for email in emails {
                group
                    .recipients
                    .retain(|r| r.email.as_clear_text_str() != email.as_ref())
            }
        }
    }

    /// Get all recipients.
    pub fn recipients(&self) -> &[Recipient] {
        &self.recipients
    }

    pub fn into_recipients(self) -> Vec<Recipient> {
        self.recipients
    }

    fn find_group_mut(&mut self, group_name: &NonEmptyString) -> Option<&mut GroupRecipient> {
        for r in self.recipients.iter_mut() {
            if let Recipient::Group(recipient) = r
                && &recipient.group_name == group_name
            {
                return Some(recipient);
            }
        }

        None
    }

    /// Create a new message recipient list fom the current state.
    ///
    /// Invalid recipients are ignored.
    pub fn to_message_recipients(&self) -> Vec<MessageRecipient> {
        let mut recipients = Vec::with_capacity(self.recipients.len());
        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(single) => {
                    let is_proton = match single.state {
                        ValidationState::Valid(is_proton) => is_proton,
                        ValidationState::Validating | ValidationState::Unchecked => false,
                        _ => continue,
                    };
                    recipients.push(MessageRecipient {
                        address: single.email.clone(),
                        is_proton,
                        name: single.display_name.clone().unwrap_or_default(),
                        group: MaybeEmptyString(None),
                    })
                }
                Recipient::Group(group) => {
                    for recipient in &group.recipients {
                        let is_proton = match recipient.state {
                            ValidationState::Valid(is_proton) => is_proton,
                            ValidationState::Validating | ValidationState::Unchecked => false,
                            _ => continue,
                        };
                        recipients.push(MessageRecipient {
                            address: recipient.email.clone(),
                            is_proton,
                            name: recipient.display_name.clone().unwrap_or_default(),
                            group: MaybeEmptyString(Some(group.group_name.clone())),
                        })
                    }
                }
            }
        }

        recipients
    }

    /// Number of recipients in this list.
    pub fn len(&self) -> usize {
        self.recipients.len()
    }

    /// Whether this recipient list is empty
    pub fn is_empty(&self) -> bool {
        self.recipients.is_empty()
    }

    fn find_recipient_by_email(&self, email: PrivateEmailRef) -> Option<&SingleRecipient> {
        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(single) => {
                    if single.email.as_ref() == email {
                        return Some(single);
                    }
                }
                Recipient::Group(group) => {
                    for recipient in &group.recipients {
                        if recipient.email.as_ref() == email {
                            return Some(recipient);
                        }
                    }
                }
            }
        }
        None
    }

    fn find_recipient_by_email_mut(
        &mut self,
        email: PrivateEmailRef,
    ) -> Option<&mut SingleRecipient> {
        for recipient in &mut self.recipients {
            match recipient {
                Recipient::Single(single) => {
                    if single.email.as_ref() == email {
                        return Some(single);
                    }
                }
                Recipient::Group(group) => {
                    for recipient in &mut group.recipients {
                        if recipient.email.as_ref() == email {
                            return Some(recipient);
                        }
                    }
                }
            }
        }
        None
    }

    fn update_recipient_validation_state(
        &mut self,
        email: PrivateEmailRef,
        state: ValidationState,
    ) {
        if let Some(recipient) = self.find_recipient_by_email_mut(email) {
            recipient.state = state;
        }
    }

    /// Check whether this list contains the given `email`.
    pub fn contains_email<'e>(&self, email: impl Into<PrivateEmailRef<'e>>) -> bool {
        self.find_recipient_by_email(email.into()).is_some()
    }

    /// Check whether this list contains all the given `emails`.
    pub fn contains_emails<'e, T: Into<PrivateEmailRef<'e>>>(
        &self,
        emails: impl IntoIterator<Item = T>,
    ) -> bool {
        for email in emails {
            if self.contains_email(email.into()) {
                return true;
            }
        }
        false
    }

    fn get_or_create_group(&mut self, group_name: NonEmptyString) -> &mut GroupRecipient {
        // Still can't do get or insert properly due to false positive
        // in borrow checker, so do the index trick.
        let position = self.recipients.iter().position(|r| {
            if let Recipient::Group(group) = r {
                return group_name == group.group_name;
            }

            false
        });

        let recipient = if let Some(position) = position {
            &mut self.recipients[position]
        } else {
            let group = GroupRecipient {
                recipients: vec![],
                group_name,
                total_in_group: 0,
            };
            self.recipients.push(Recipient::Group(group));
            self.recipients.last_mut().expect("recipients must exist")
        };
        match recipient {
            Recipient::Group(group) => group,
            _ => unreachable!(),
        }
    }

    fn is_duplicate_address(&self, email: PrivateEmailRef) -> bool {
        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(r) => {
                    if r.email.as_ref() == email {
                        return true;
                    }
                }
                Recipient::Group(g) => {
                    for recipient in &g.recipients {
                        if recipient.email.as_ref() == email {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn validate_expiration_feature(&self, report: &mut ExpirationFeatureSupportReport) {
        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(r) => report.check(r.email.as_ref(), r.state),
                Recipient::Group(group) => {
                    for recipient in &group.recipients {
                        report.check(recipient.email.as_ref(), recipient.state)
                    }
                }
            }
        }
    }

    pub fn fill_expiration_support_report_as_supported(
        &self,
        report: &mut ExpirationFeatureSupportReport,
    ) {
        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(r) => report.add_as_supported(r.email.as_ref(), r.state),
                Recipient::Group(group) => {
                    for recipient in &group.recipients {
                        report.add_as_supported(recipient.email.as_ref(), recipient.state)
                    }
                }
            }
        }
    }
}

pub struct RecipientValidationUpdate {
    updates: Vec<(PrivateEmail, ValidationState)>,
}

impl RecipientValidationUpdate {
    pub fn apply(self, list: &mut RecipientList) {
        for (email, state) in self.updates {
            list.update_recipient_validation_state(email.as_ref(), state);
        }
    }
}

/// Specifies the behaviour for the mechanism through which updates are notified.
pub trait OnBackgroundValidationComplete: Send + Sync + Clone + 'static {
    fn recipients_validation_state_updated(
        &self,
        updates: RecipientValidationUpdate,
    ) -> impl Future<Output = ()> + Send;
}

/// Channel based background validation updates.
#[derive(Clone)]
pub struct ChannelBackgroundValidationComplete(flume::Sender<RecipientValidationUpdate>);

impl ChannelBackgroundValidationComplete {
    pub fn new(capacity: usize) -> (Self, flume::Receiver<RecipientValidationUpdate>) {
        let (sender, receiver) = flume::bounded(capacity);
        (Self(sender), receiver)
    }
}

impl OnBackgroundValidationComplete for ChannelBackgroundValidationComplete {
    async fn recipients_validation_state_updated(&self, updates: RecipientValidationUpdate) {
        let _ = self.0.send_async(updates).await;
    }
}

/// This version of a recipient list validates recipient addresses in the background when
/// they are added to the list.
///
/// Background validation is performed via async tasks. Once validation finishes the list is
/// updated in place and user is notified via the provided
/// [`OnBackgroundValidationComplete`] implementation.
///
/// This type exists so that the UI layer can defer the validation of the addresses as the user
/// types them.
pub struct ValidatingRecipientList<'l, T: OnBackgroundValidationComplete> {
    list: &'l mut RecipientList,
    cancellation_token: CancellationToken,
    cb: T,
}
impl<'l, T: OnBackgroundValidationComplete> ValidatingRecipientList<'l, T> {
    /// Create a new instance.
    pub fn new(
        cancellation_token: CancellationToken,
        list: &'l mut RecipientList,
        on_updated: T,
    ) -> Self {
        Self {
            list,
            cb: on_updated,
            cancellation_token,
        }
    }

    pub fn check_all(&mut self, ctx: &MailUserContext) {
        let mut emails_to_validate = Vec::new();
        let mut check_recipient = |recipient: &mut SingleRecipient| {
            if recipient.state == ValidationState::Unchecked {
                recipient.state = ValidationState::Validating;
                emails_to_validate.push(recipient.email.clone());
            }
        };
        for recipient in &mut self.list.recipients {
            match recipient {
                Recipient::Single(recipient) => {
                    check_recipient(recipient);
                }
                Recipient::Group(group) => {
                    for recipient in &mut group.recipients {
                        check_recipient(recipient);
                    }
                }
            }
        }
        self.validate_addresses(ctx, emails_to_validate);
    }

    /// See [`RecipientList::add_single`] for more details.
    pub fn add_single(
        &mut self,
        ctx: &MailUserContext,
        entry: RecipientEntry,
    ) -> Result<(), RecipientError> {
        let entry = self.list.add_single(entry)?;
        let emails = if entry.state == ValidationState::Unchecked {
            entry.state = ValidationState::Validating;
            vec![entry.email.clone()]
        } else {
            vec![]
        };
        self.validate_addresses(ctx, emails);

        Ok(())
    }

    /// See [`RecipientList::add_group`] for more details.
    pub fn add_group(
        &mut self,
        ctx: &MailUserContext,
        group_name: NonEmptyString,
        entries: impl IntoIterator<Item = RecipientEntry>,
        total_in_group: u64,
    ) -> Vec<RecipientEntry> {
        let (group, duplicates) = self.list.add_group(group_name, entries, total_in_group);

        let to_validate = group
            .recipients
            .iter_mut()
            .filter_map(|r| {
                if r.state == ValidationState::Unchecked {
                    r.state = ValidationState::Validating;
                    Some(r.email.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        self.validate_addresses(ctx, to_validate);

        duplicates
    }

    fn validate_addresses(&self, ctx: &MailUserContext, to_validate: Vec<PrivateEmail>) {
        if to_validate.is_empty() {
            return;
        }
        let cb = self.cb.clone();
        let ctx = ctx.as_arc();
        let ctx_cloned = Arc::clone(&ctx);
        ctx_cloned
            .mail_context()
            .core_context()
            .task_service()
            .spawn_cancellable(self.cancellation_token.clone(), async move {
                let mut update_statuses = Vec::with_capacity(to_validate.len());
                for email in to_validate {
                    let status = validate_address(&ctx, email.clone()).await;
                    update_statuses.push((email, status));
                }

                cb.recipients_validation_state_updated(RecipientValidationUpdate {
                    updates: update_statuses,
                })
                .await;
            });
    }
}

/// Validates an address using the get keys route for the given `email`.
///
/// Network failures do not result in errors, but return [`ValidationState::Unchecked`] instead.
///
async fn validate_address(ctx: &MailUserContext, email: PrivateEmail) -> ValidationState {
    let options = GetKeysAllOptions {
        email: email.clone(),
        internal_only: Some(false),
    };

    let state = match ctx.user_context().session().get_keys_all(options).await {
        Ok(response) => ValidationState::Valid(response.is_proton),
        Err(e) => ValidationState::from(e),
    };
    tracing::debug!("Validation state updated for {email}: {state:?}");
    state
}

const PROTON_EMAIL_DOMAINS: [&str; 6] = [
    "@proton.me",
    "@protonmail.ch",
    "@protonmail.com",
    "@pm.me",
    "@proton.ch",
    "@external.proton.ch",
];
