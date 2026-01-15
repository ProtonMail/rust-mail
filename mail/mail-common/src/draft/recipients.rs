use crate::MailUserContext;
use crate::datatypes::MessageRecipient;
use crate::models::{DraftMetadata, MailSettings, MessageReplyTo, MetadataId};
use email_address::EmailAddress;
use non_empty_string::NonEmptyString;
use proton_core_api::consts::CoreBundle;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{PrivateEmail, PrivateEmailRef, PrivateString};
use proton_core_common::models::ContactEmail;
use proton_core_common::{CoreContextError, PublicAddressKeyFetchPolicy};
use proton_crypto_account::keys::{EmailMimeType, RecipientType};
use proton_crypto_inbox::keys::ComposerPreference;
use proton_crypto_inbox::lock_icon::UiLock;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Tether;
use std::collections::HashSet;
use std::future::Future;
use std::str::FromStr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

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

    pub fn into_string(self) -> String {
        self.0.map(NonEmptyString::into_inner).unwrap_or_default()
    }
}

impl From<String> for MaybeEmptyString {
    fn from(value: String) -> Self {
        Self(NonEmptyString::try_from(value).ok())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ValidationState {
    Valid { official: bool, proton: bool },
    DoesNotExist,
    InvalidEmail,
    Unchecked,
    Validating,
    Unknown,
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum PrivacyLockState {
    #[default]
    Default,
    Calculating,
    Calculated(Option<UiLock>),
}

impl PrivacyLockState {
    #[must_use]
    pub fn should_recalculate(&self) -> bool {
        matches!(self, Self::Default)
    }

    #[must_use]
    pub fn as_ui_lock(&self) -> Option<UiLock> {
        match self {
            PrivacyLockState::Default | PrivacyLockState::Calculating => None,
            PrivacyLockState::Calculated(lock) => *lock,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleRecipient {
    pub display_name: Option<PrivateString>,
    pub email: PrivateEmail,
    pub state: ValidationState,
    #[serde(skip)] // TODO(@Leander): Decouple draft action types from this list
    pub privacy_lock: PrivacyLockState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupRecipient {
    pub recipients: Vec<SingleRecipient>,
    pub group_name: NonEmptyString,
    pub total_in_group: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientError {
    #[error("Address {0} already exists in the recipient list")]
    DuplicateAddress(PrivateEmail),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Recipient {
    Single(SingleRecipient),
    Group(GroupRecipient),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RecipientEntry {
    pub name: Option<PrivateString>,
    pub email: PrivateEmail,
}

impl RecipientEntry {
    /// Parses given string as an email recipient.
    ///
    /// Basically:
    ///
    /// - `eoj@pm.me` ends up as `name: None` and `email: "eoj@pm.me"`,
    /// - `joe <eoj@pm.me>` ends up as `name: Some("joe")` and `email: "eoj@pm.me"`.
    pub fn new(email: &str) -> Self {
        let email = email.trim();

        if let Ok(email) = EmailAddress::from_str(email) {
            let name = email.display_part().trim();

            if !name.is_empty() {
                return Self {
                    name: Some(name.into()),
                    email: email.email().into(),
                };
            }
        }

        Self {
            name: None,
            email: email.into(),
        }
    }
}

pub trait ContactGroupResolver {
    /// Returns the number of members in given contact group or `None` if no
    /// such group exists.
    fn resolve_contact_group_total(
        &self,
        name: &NonEmptyString,
    ) -> impl Future<Output = Option<u64>>;
}

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
            ValidationState::Valid { proton, .. } => {
                if proton || is_known_proton_domain(email.clone()) {
                    self.supported.insert(email.to_owned());
                } else {
                    self.unsupported.insert(email.to_owned());
                }
            }
            _ => {
                // If we are unable to validate at the moment, we can still
                // quickly validate if some address ends in a known domain as they are
                // always supported.
                if is_known_proton_domain(email.clone()) {
                    self.supported.insert(email.to_owned());
                    return;
                }
                self.unknown.insert(email.to_owned());
            }
        };
    }

    fn add_as_supported(&mut self, email: PrivateEmailRef, validation_state: ValidationState) {
        match validation_state {
            ValidationState::Valid { proton, .. } => {
                if proton {
                    self.supported.insert(email.to_owned());
                }
            }
            ValidationState::Unchecked | ValidationState::Validating => {
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
#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecipientList {
    recipients: Vec<Recipient>,
}

impl RecipientList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(recipients: impl IntoIterator<Item = Recipient>) -> Self {
        Self {
            recipients: recipients.into_iter().collect(),
        }
    }

    pub async fn from_message_recipients(
        contact_group_resolver: &impl ContactGroupResolver,
        recipients: impl IntoIterator<Item = MessageRecipient>,
    ) -> Self {
        let mut list = Self::new();

        for recipient in recipients {
            let entry = RecipientEntry {
                email: recipient.address,
                name: if recipient.name.is_empty() {
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
                name: if recipient.name.is_empty() {
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
            display_name: entry.name,
            email: entry.email,
            state,
            privacy_lock: PrivacyLockState::default(),
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
                display_name: recipient.name,
                email: recipient.email,
                state,
                privacy_lock: PrivacyLockState::default(),
            });
        }

        let group = self.get_or_create_group(group_name);
        group.total_in_group = total_in_group;
        group.recipients.extend(recipients);
        (group, duplicates)
    }

    pub fn remove_group(&mut self, group_name: &NonEmptyString) {
        self.recipients.retain(|r| {
            let Recipient::Group(recipient) = r else {
                return true;
            };

            group_name != &recipient.group_name
        })
    }

    pub fn remove_group_recipient(&mut self, group_name: &NonEmptyString, email: &str) {
        self.remove_group_recipients(group_name, std::iter::once(email));
    }

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

    pub fn to_message_recipients(&self) -> Vec<MessageRecipient> {
        let mut recipients = Vec::with_capacity(self.recipients.len());

        for recipient in &self.recipients {
            match recipient {
                Recipient::Single(single) => {
                    let is_proton = match single.state {
                        ValidationState::Valid { official, .. } => official,
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
                            ValidationState::Valid { official, .. } => official,
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

    pub fn len(&self) -> usize {
        self.recipients.len()
    }

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

    fn update_recipient_privacy_lock(
        &mut self,
        email: PrivateEmailRef,
        privacy_lock: PrivacyLockState,
    ) {
        if let Some(recipient) = self.find_recipient_by_email_mut(email) {
            recipient.privacy_lock = privacy_lock;
        }
    }

    pub fn contains_email<'e>(&self, email: impl Into<PrivateEmailRef<'e>>) -> bool {
        self.find_recipient_by_email(email.into()).is_some()
    }

    pub fn contains_emails<'e, T>(&self, emails: impl IntoIterator<Item = T>) -> bool
    where
        T: Into<PrivateEmailRef<'e>>,
    {
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

#[derive(Default)]
pub struct RecipientPrivacyLockUpdate {
    updates: Vec<(PrivateEmail, PrivacyLockState)>,
}

impl RecipientPrivacyLockUpdate {
    pub fn apply(self, list: &mut RecipientList) {
        for (email, lock) in self.updates {
            list.update_recipient_privacy_lock(email.as_ref(), lock);
        }
    }
}

pub trait OnBackgroundValidationComplete: Send + Sync + Clone + 'static {
    fn recipients_validation_state_updated(
        &self,
        updates: RecipientValidationUpdate,
    ) -> impl Future<Output = ()> + Send;
}

pub trait OnPrivacyLockUpdate: Send + Sync + Clone + 'static {
    fn recipient_privacy_lock_updated(
        &self,
        updates: RecipientPrivacyLockUpdate,
    ) -> impl Future<Output = ()> + Send;
}

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
pub struct ValidatingRecipientList<'l, T: OnBackgroundValidationComplete + OnPrivacyLockUpdate> {
    list: &'l mut RecipientList,
    cancellation_token: CancellationToken,
    lock_cancellation_token: CancellationToken,
    cb: T,
    draft_id: MetadataId,
    mime_type: EmailMimeType,
    is_byoe: bool,
}

impl<'l, T: OnBackgroundValidationComplete + OnPrivacyLockUpdate> ValidatingRecipientList<'l, T> {
    pub fn new(
        cancellation_token: CancellationToken,
        lock_cancellation_token: CancellationToken,
        list: &'l mut RecipientList,
        on_updated: T,
        draft_id: MetadataId,
        mime_type: EmailMimeType,
        is_byoe: bool,
    ) -> Self {
        Self {
            list,
            cb: on_updated,
            cancellation_token,
            lock_cancellation_token,
            draft_id,
            mime_type,
            is_byoe,
        }
    }

    pub fn check_all(&mut self, ctx: &MailUserContext) {
        let mut emails_to_validate = Vec::new();
        let mut locks_to_calculate = Vec::new();

        let mut check_recipient = |recipient: &mut SingleRecipient| {
            if recipient.state == ValidationState::Unchecked {
                recipient.state = ValidationState::Validating;
                emails_to_validate.push(recipient.email.clone());
            }

            if recipient.privacy_lock.should_recalculate() {
                recipient.privacy_lock = PrivacyLockState::Calculating;
                locks_to_calculate.push(recipient.email.clone());
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
        self.calculate_privacy_locks(ctx, locks_to_calculate);
    }

    pub fn recalculate_all_privacy_locks(&mut self, ctx: &MailUserContext) {
        let mut locks_to_calculate = Vec::new();

        let mut check_recipient = |recipient: &mut SingleRecipient| {
            recipient.privacy_lock = PrivacyLockState::Calculating;
            locks_to_calculate.push(recipient.email.clone());
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

        self.calculate_privacy_locks(ctx, locks_to_calculate);
    }

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

        let to_calculate = if entry.privacy_lock.should_recalculate() {
            entry.privacy_lock = PrivacyLockState::Calculating;
            vec![entry.email.clone()]
        } else {
            vec![]
        };

        self.validate_addresses(ctx, emails);
        self.calculate_privacy_locks(ctx, to_calculate);

        Ok(())
    }

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

        let to_calculate = group
            .recipients
            .iter_mut()
            .filter_map(|r| {
                if r.privacy_lock.should_recalculate() {
                    r.privacy_lock = PrivacyLockState::Calculating;
                    Some(r.email.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        self.validate_addresses(ctx, to_validate);
        self.calculate_privacy_locks(ctx, to_calculate);

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

    fn calculate_privacy_locks(&self, ctx: &MailUserContext, to_calculate: Vec<PrivateEmail>) {
        if to_calculate.is_empty() {
            return;
        }

        let cb = self.cb.clone();
        let ctx = ctx.as_arc();

        // When using byoe we don't display any privacy locks
        if self.is_byoe {
            ctx.mail_context()
                .core_context()
                .task_service()
                .spawn_cancellable(self.lock_cancellation_token.clone(), async move {
                    let updates = RecipientPrivacyLockUpdate {
                        updates: to_calculate
                            .into_iter()
                            .map(|e| (e, PrivacyLockState::Calculated(None)))
                            .collect(),
                    };
                    cb.recipient_privacy_lock_updated(updates).await;
                });
        } else {
            let ctx_cloned = Arc::clone(&ctx);
            let draft_id = self.draft_id;
            let mime_type = self.mime_type;

            ctx_cloned
                .mail_context()
                .core_context()
                .task_service()
                .spawn_cancellable(self.lock_cancellation_token.clone(), async move {
                    let updates =
                        calculate_privacy_locks(&ctx, draft_id, mime_type, to_calculate).await;
                    cb.recipient_privacy_lock_updated(updates).await;
                });
        }
    }
}

/// Validates an address using the get keys route for the given `email`.
///
/// Network failures do not result in errors, but return [`ValidationState::Unchecked`] instead.
///
async fn validate_address(ctx: &MailUserContext, email: PrivateEmail) -> ValidationState {
    let pgp_provider = new_pgp_provider();
    let state = match ctx
        .user_context()
        .public_address_keys(
            &pgp_provider,
            email.as_ref(),
            false,
            PublicAddressKeyFetchPolicy::AllowCachedFallback,
        )
        .await
    {
        Ok(keys) => ValidationState::Valid {
            official: keys.is_proton,
            proton:
            // if it's a known proton domain we can skip the key check
            if is_known_proton_domain(email.as_ref()) {
                true
            } else {
                // check whether this domain is actually a proton powered email account
                keys.into_inbox_keys(true).recipient_type == RecipientType::Internal
            },
        },
        Err(CoreContextError::Api(e)) => ValidationState::from(e),
        Err(e) => {
            error!("Unknown validation error: {e:?}");
            ValidationState::Unknown
        }
    };

    tracing::debug!("Validation state updated for {email}: {state:?}");
    state
}

async fn calculate_privacy_locks(
    ctx: &MailUserContext,
    draft_id: MetadataId,
    mime_type: EmailMimeType,
    emails: Vec<PrivateEmail>,
) -> RecipientPrivacyLockUpdate {
    let Ok(mut tether) = ctx.user_stash().connection().await else {
        warn!("Failed to acquire db connection");
        return RecipientPrivacyLockUpdate {
            updates: emails
                .into_iter()
                .map(|email| (email, PrivacyLockState::default()))
                .collect(),
        };
    };

    let Ok(Some(metadata)) = DraftMetadata::load(draft_id, &tether).await else {
        warn!("Failed to load draft metadata");
        return RecipientPrivacyLockUpdate {
            updates: emails
                .into_iter()
                .map(|email| (email, PrivacyLockState::default()))
                .collect(),
        };
    };

    let mail_settings = MailSettings::get_or_default(&tether).await;

    let composer_preference = ComposerPreference {
        encrypt_to_outside: metadata.password.is_some(),
        composer_body_mime_type: mime_type,
    };

    let mut updates = Vec::with_capacity(emails.len());
    for email in emails {
        let lock = calculate_privacy_lock(
            ctx,
            email.as_ref(),
            &mail_settings,
            composer_preference,
            &mut tether,
        )
        .await;

        updates.push((email, lock));
    }

    RecipientPrivacyLockUpdate { updates }
}

async fn calculate_privacy_lock(
    ctx: &MailUserContext,
    email: PrivateEmailRef<'_>,
    mail_settings: &MailSettings,
    composer_preference: ComposerPreference,
    tether: &mut Tether,
) -> PrivacyLockState {
    let pgp_provider = new_pgp_provider();
    match ctx
        .recipient_send_preferences(
            &pgp_provider,
            tether,
            email,
            mail_settings.crypto_mail_settings(),
            composer_preference,
            proton_core_common::AddressKeysContactFetchPolicy::AllowCachedFallback,
        )
        .await
    {
        Ok(send_prefs) => PrivacyLockState::Calculated(UiLock::for_composer(&send_prefs)),
        Err(e) => {
            warn!("Failed to fetch sender preferences: {e}");
            PrivacyLockState::Default
        }
    }
}

const PROTON_EMAIL_DOMAINS: [&str; 6] = [
    "@proton.me",
    "@protonmail.ch",
    "@protonmail.com",
    "@pm.me",
    "@proton.ch",
    "@external.proton.ch",
];

fn is_known_proton_domain(email: PrivateEmailRef) -> bool {
    for domain in PROTON_EMAIL_DOMAINS {
        if email.as_clear_text_str().to_lowercase().ends_with(domain) {
            return true;
        }
    }

    false
}
