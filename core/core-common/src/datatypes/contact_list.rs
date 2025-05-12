use super::avatar::AvatarInformation;
use crate::datatypes::{LabelType, LocalContactEmailId, LocalContactId, LocalLabelId};
use crate::models::{Contact, ContactEmail, Label};
use crate::utils::MapVec as _;
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::mem;
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_GROUP: &str = "#";

/// This is the main data structure that is used to represent the group of contacts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupedContacts {
    /// The field represent first grapheme of the name of the contact
    pub grouped_by: String,

    // The field represent the list of contacts or groups for the given grapheme
    pub items: Vec<ContactItemType>,
}

impl GroupedContacts {
    /// Builds grouped contacts based on flat contact list and contact groups
    ///
    /// # Contact groups
    ///
    /// Note, that the contact group is represented by [`Label`]. Currently, this function WON'T
    /// assert if the label has type `ContactGroup`.
    ///
    /// # Panics
    ///
    /// This function may panic if the contact group does not have local ID assigned.
    ///
    #[must_use]
    pub fn from_contacts_and_groups(
        contacts: Vec<Contact>,
        contact_groups: Vec<Label>,
    ) -> Vec<Self> {
        debug_assert!(
            contact_groups
                .iter()
                .all(|group| group.label_type == LabelType::ContactGroup)
        );

        let mut contact_group_items: HashMap<LabelId, ContactGroupItem> = contact_groups
            .into_iter()
            .filter(|group| group.label_type == LabelType::ContactGroup)
            .map(|group| {
                (
                    group.remote_id.unwrap().clone(),
                    ContactGroupItem {
                        local_id: group.local_id.unwrap(),
                        name: group.name.clone(),
                        avatar_information: AvatarInformation::from(&group.name),
                        contacts: vec![],
                    },
                )
            })
            .collect();

        let contact_items = contacts
            .into_iter()
            .sorted_by(|one, other| {
                let one_words: String = one.name.unicode_words().collect();
                let other_words: String = other.name.unicode_words().collect();
                one_words.cmp(&other_words)
            })
            .map(|contact| {
                let item = ContactItem::from(contact.clone());
                contact.label_ids.iter().for_each(|id| {
                    if let Some(group) = contact_group_items.get_mut(id) {
                        group.contacts.push(item.clone());
                    }
                });
                item
            })
            .collect::<Vec<_>>();

        let mut btmap: BTreeMap<String, Vec<ContactItemType>> = BTreeMap::new();
        contact_items
            .into_iter()
            .map_into::<ContactItemType>()
            .chain(
                contact_group_items
                    .into_values()
                    .map_into::<ContactItemType>(),
            )
            .for_each(|contact| {
                let key = contact.key();
                let key = if key.is_empty() || key == "?" {
                    DEFAULT_GROUP
                } else {
                    key
                };

                btmap.entry(key.to_owned()).or_default().push(contact);
            });

        btmap
            .into_iter()
            .map(|(grouped_by, items)| GroupedContacts { grouped_by, items })
            .collect()
    }
}

/// List of contacts is composed of contacts and groups.
/// This enum is used to represent the either one.
#[derive(Clone, Debug, PartialEq)]
pub enum ContactItemType {
    Contact(ContactItem),
    Group(ContactGroupItem),
}

impl ContactItemType {
    /// Represents the first grapheme in the contact list, used to sort the contacts alphabetically
    fn key(&self) -> &str {
        let avatar_information = match self {
            ContactItemType::Contact(contact_item) => &contact_item.avatar_information,
            ContactItemType::Group(contact_group_item) => &contact_group_item.avatar_information,
        };

        avatar_information.text.as_str()
    }
}

impl From<ContactItem> for ContactItemType {
    fn from(value: ContactItem) -> Self {
        Self::Contact(value)
    }
}

impl From<ContactGroupItem> for ContactItemType {
    fn from(value: ContactGroupItem) -> Self {
        Self::Group(value)
    }
}

/// This is the main data structure that is used to represent the contact.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactItem {
    /// The field represent the unique identifier of the contact in the database
    pub local_id: LocalContactId,

    /// The field represent the name of the contact
    pub name: String,

    /// The field represent the avatar information of the contact
    pub avatar_information: AvatarInformation,

    /// The field represent the list of emails of the contact
    pub emails: Vec<ContactEmailItem>,
}

impl From<Contact> for ContactItem {
    fn from(value: Contact) -> Self {
        Self {
            local_id: value.local_id.unwrap(),
            avatar_information: AvatarInformation::from(&value.name)
                .or_else(
                    value
                        .contact_emails
                        .first()
                        .map(|email| email.email.as_str())
                        .unwrap_or_default(),
                )
                .or_else_unchecked("?"),
            emails: value.contact_emails.map_vec(),
            name: value.name,
        }
    }
}

/// This is the main data structure that is used to represent the contact group.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactGroupItem {
    /// The field represent the unique identifier of the contact group in the database
    pub local_id: LocalLabelId,

    /// The field represent the name of the contact group
    pub name: String,

    /// The field represent the avatar information of the contact group
    pub avatar_information: AvatarInformation,

    /// The field represent the list of emails of the contact group
    pub contacts: Vec<ContactItem>,
}

/// This is the main data structure that is used to represent the contact email.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactEmailItem {
    /// The field represent the unique identifier of the contact email in the database
    pub local_id: LocalContactEmailId,

    /// The field represent the email of the contact
    pub email: String,

    /// The field represent if the email is a proton email
    pub is_proton: bool,

    /// The field represent the last used time of the email
    pub last_used_time: u64,
}

impl From<ContactEmail> for ContactEmailItem {
    fn from(value: ContactEmail) -> Self {
        Self {
            local_id: value.local_id.unwrap(),
            email: value.email,
            is_proton: value.is_proton,
            last_used_time: value.last_used_time,
        }
    }
}

/// Device contact feeded by the mobile/web application.
/// Used as an input for generating list of contact suggestions ([`ContactSuggestion`])
///
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceContact {
    /// The field represents unique key identifier used by the user to distinguish elements in the array
    pub key: String,

    /// The field represents the name of the contact
    pub name: String,

    /// List of email addresses assigned to the contact. That list has an arbitrary order given by the user
    pub emails: Vec<String>,
}

/// Collection of sorted contact suggestions
#[derive(Debug, PartialEq)]
pub struct ContactSuggestions {
    /// Sorted and deduplicated suggestions
    suggestions: Vec<ContactSuggestion>,
}

impl From<Vec<ContactSuggestion>> for ContactSuggestions {
    fn from(suggestions: Vec<ContactSuggestion>) -> Self {
        Self { suggestions }
    }
}

impl ContactSuggestions {
    /// Build contact suggestion list that is sorted and deduplicated
    ///
    /// # Contact groups
    ///
    /// Note, that the contact group is represented by [`Label`]. Currently, this function WON'T
    /// assert if the label has type `ContactGroup`.
    ///
    /// # Filtering
    ///
    /// This function does not filter the results. Make sure, that the filtering
    /// does not exclude contacts that are part of contact group still matching the query.
    ///
    /// # Panics
    ///
    /// When contact has no local id (meaning it was not fetched from the database).
    /// Or when contact group has no remote ID
    ///
    #[must_use]
    pub fn from_contacts_and_device_contacts(
        contacts: Vec<Contact>,
        contact_groups: Vec<Label>,
        device_contacts: Vec<DeviceContact>,
    ) -> Self {
        debug_assert!(
            contact_groups
                .iter()
                .all(|group| group.label_type == LabelType::ContactGroup)
        );

        let label_ids = contacts
            .iter()
            .flat_map(|contact| {
                contact.label_ids.iter().cloned().chain(
                    contact
                        .contact_emails
                        .iter()
                        .flat_map(|email| email.label_ids.iter().cloned()),
                )
            })
            .collect::<HashSet<_>>();

        let mut contact_groups: HashMap<LabelId, ContactGroup> = contact_groups
            .into_iter()
            .filter(|group| group.label_type == LabelType::ContactGroup)
            // TODO(ET-2030): We should not reference groups by remote ids, instead we should use local ids
            // This is to ensure the offline mode works with contacts and contact groups not synced with API
            .filter(|group| label_ids.contains(group.remote_id.as_ref().unwrap()))
            .map(|group| {
                (
                    group.remote_id.unwrap(),
                    ContactGroup {
                        key: format!("group/{}", group.local_id.unwrap()),
                        name: group.name.clone(),
                        emails: vec![],
                    },
                )
            })
            .collect();

        let proton_suggestions: Vec<_> = contacts
            .into_iter()
            .filter(|contact| !contact.deleted)
            .flat_map(|contact| {
                contact
                    .contact_emails
                    .clone()
                    .into_iter()
                    .map(move |email| (contact.clone(), email))
            })
            .sorted_by_key(|(contact, email)| {
                // sorted_by_key is using ASC order. By making negative boolean or subtracting the time
                // we ensure it is ordered by first proton mails and then by latest mails
                // `last_used_time` is u64, to ensure that
                (
                    !email.is_proton,
                    u64::MAX - email.last_used_time,
                    email.email.unicode_words().collect::<String>(),
                    contact.name.clone(),
                )
            })
            .map(|(contact, email)| {
                Self::aggregate_emails_to_groups(&mut contact_groups, contact, email)
            })
            .map(|(contact, email)| ContactSuggestion::new_contact(contact, email))
            .collect();

        let rest = contact_groups
            .into_values()
            .filter(|group| !group.emails.is_empty())
            .map(ContactSuggestion::new_group)
            .chain(
                device_contacts
                    .into_iter()
                    .map(ContactSuggestion::new_device_contact),
            )
            .sorted()
            .flat_map(|suggestion| match suggestion {
                FollowingSuggestion::ContactGroup(contact_suggestion) => vec![contact_suggestion],
                FollowingSuggestion::DeviceContact { suggestions, .. } => suggestions,
            });

        Self::concat_iters(proton_suggestions, rest)
    }

    pub fn concat(&mut self, other: Self) {
        let mut suggestions = vec![];
        mem::swap(&mut self.suggestions, &mut suggestions);
        *self = Self::concat_iters(suggestions, other.suggestions);
    }

    fn concat_iters(
        one: impl IntoIterator<Item = ContactSuggestion>,
        other: impl IntoIterator<Item = ContactSuggestion>,
    ) -> Self {
        Self {
            suggestions: one
                .into_iter()
                .chain(other)
                .unique_by(|suggestion| {
                    suggestion
                        .email()
                        .map(ToOwned::to_owned)
                        .map_or_else(|| suggestion.key.clone(), |email| email.to_lowercase())
                })
                .collect(),
        }
    }

    /// Return all contact suggestions
    ///
    #[must_use]
    pub fn all(&self) -> &[ContactSuggestion] {
        &self.suggestions
    }

    /// Return suggestions filtered by the query.
    ///
    #[must_use]
    pub fn filtered(&self, query: &str) -> Vec<ContactSuggestion> {
        let query = query.trim();
        let query = query.to_lowercase();

        // Early exit heurestic
        if query.is_empty() {
            return Vec::new();
        }

        self.suggestions
            .iter()
            .filter(|suggestion| {
                suggestion.name.to_lowercase().contains(&query)
                    || suggestion
                        .email()
                        .is_some_and(|email| email.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }

    fn aggregate_emails_to_groups(
        contact_groups: &mut HashMap<LabelId, ContactGroup>,
        contact: Contact,
        mut email: ContactEmail,
    ) -> (Contact, ContactEmailItem) {
        let label_ids = mem::take(&mut email.label_ids);
        let email = ContactEmailItem::from(email);
        for label_id in label_ids.iter() {
            if let Some(group) = contact_groups.get_mut(label_id) {
                group.emails.push(email.clone());
            }
        }
        (contact, email)
    }
}

/// Used in the composer to suggest email addresses based on the user input (To:, CC: etc fields)
/// Contrary to the [`ContactItemType`] it also might be a device contact
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactSuggestion {
    /// The field represents unique key identifier used by the user to distinguish elements in the array
    pub key: String,

    /// The field represents the name of the contact
    pub name: String,

    /// The field represents the avatar information of the contact
    pub avatar_information: AvatarInformation,

    /// The kind of contact suggestion. Whether it is a native contact, proton contact or a group.
    pub kind: ContactSuggestionKind,
}

impl ContactSuggestion {
    /// Returns an email linked to the suggestion. If it suggests contact group, it returns `None`
    ///
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        match &self.kind {
            ContactSuggestionKind::ContactItem(contact_email_item) => {
                Some(contact_email_item.email.as_str())
            }
            ContactSuggestionKind::DeviceContact(device_contact_suggestion) => {
                Some(device_contact_suggestion.email.as_str())
            }
            ContactSuggestionKind::ContactGroup(_) => None,
        }
    }

    fn new_group(group: ContactGroup) -> FollowingSuggestion {
        FollowingSuggestion::ContactGroup(Self {
            key: group.key,
            avatar_information: AvatarInformation::from(&group.name),
            name: group.name,
            kind: ContactSuggestionKind::ContactGroup(group.emails),
        })
    }

    fn new_contact(contact: Contact, email: ContactEmailItem) -> Self {
        Self {
            key: format!("contact/{}", email.local_id),
            avatar_information: AvatarInformation::from(&contact.name),
            name: contact.name,
            kind: ContactSuggestionKind::ContactItem(email),
        }
    }

    fn new_device_contact(contact: DeviceContact) -> FollowingSuggestion {
        FollowingSuggestion::DeviceContact {
            key: contact.key.clone(),
            name: contact.name.clone(),
            suggestions: contact
                .emails
                .into_iter()
                .enumerate()
                .map(|(idx, email)| Self {
                    key: format!("device-contact-email/{}-{}", contact.key, idx),
                    avatar_information: AvatarInformation::from(&contact.name),
                    name: contact.name.clone(),
                    kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion { email }),
                })
                .collect(),
        }
    }
}

struct ContactGroup {
    key: String,
    name: String,
    emails: Vec<ContactEmailItem>,
}

/// A suggestion that is not based on the proton contact
/// This type is required for some custom ordering logic
///
#[derive(Debug)]
enum FollowingSuggestion {
    /// Suggestion represents contact group
    ContactGroup(ContactSuggestion),
    /// Multiple suggestions coming from the same device contact
    DeviceContact {
        name: String,
        key: String,
        suggestions: Vec<ContactSuggestion>,
    },
}
impl Ord for FollowingSuggestion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lex_name()
            .cmp(&other.lex_name())
            .then(self.discriminant().cmp(&other.discriminant()))
            .then(self.key().cmp(other.key()))
    }
}
impl PartialEq for FollowingSuggestion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for FollowingSuggestion {}
impl PartialOrd for FollowingSuggestion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl FollowingSuggestion {
    fn lex_name(&self) -> String {
        let name = match self {
            FollowingSuggestion::ContactGroup(contact_suggestion) => &contact_suggestion.name,
            FollowingSuggestion::DeviceContact { name, .. } => name,
        };
        name.unicode_words().collect()
    }
    fn key(&self) -> &str {
        match self {
            FollowingSuggestion::ContactGroup(contact_suggestion) => {
                contact_suggestion.key.as_str()
            }
            FollowingSuggestion::DeviceContact { key, .. } => key.as_str(),
        }
    }
}

/// Kind of email suggestion
/// Note, variants of this enum are flat - that is, if one contact has assigned two emails,
/// it would be represented by two instances of [`ContactSuggestion`].
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactSuggestionKind {
    /// Proton contact, stored in the local cache and shared between user devices
    ContactItem(ContactEmailItem),
    /// A device, native contact, stored only locally on the current device.
    DeviceContact(DeviceContactSuggestion),
    /// Proton contact group, that consists only other proton contacts, and never device contact.
    ContactGroup(Vec<ContactEmailItem>),
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
enum FollowingSuggestionDiscriminant {
    DeviceContact,
    ContactGroup,
}
impl FollowingSuggestion {
    fn discriminant(&self) -> FollowingSuggestionDiscriminant {
        match self {
            FollowingSuggestion::ContactGroup(_) => FollowingSuggestionDiscriminant::ContactGroup,
            FollowingSuggestion::DeviceContact { .. } => {
                FollowingSuggestionDiscriminant::DeviceContact
            }
        }
    }
}

/// A device, native contact, stored only locally on the current device.
///
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceContactSuggestion {
    /// The field represents the email address used in the device contact
    pub email: String,
}
