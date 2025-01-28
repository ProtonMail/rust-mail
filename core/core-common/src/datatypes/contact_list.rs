use super::avatar::AvatarInformation;
use crate::datatypes::{LabelType, LocalContactEmailId, LocalContactId, LocalLabelId};
use crate::models::{Contact, ContactEmail, Label};
use crate::utils::MapVec as _;
use itertools::Itertools;
use proton_api_core::services::proton::common::LabelId;
use std::collections::{BTreeMap, HashMap};
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
        debug_assert!(contact_groups
            .iter()
            .all(|group| group.label_type == LabelType::ContactGroup));

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
#[derive(Clone, Debug, PartialEq)]
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
pub struct DeviceContact {
    /// The field represents unique key identifier used by the user to distinguish elements in the array
    pub key: String,

    /// The field represents the name of the contact
    pub name: String,

    /// List of email addresses assigned to the contact. That list has an arbitrary order given by the user
    pub emails: Vec<String>,
}

/// Used in the composer to suggest email addresses based on the user input (To:, CC: etc fields)
/// Contrary to the [`ContactItemType`] it also might be a device contact
///
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
    /// Build contact suggestion list that is sorted and deduplicated
    ///
    /// # Contact groups
    ///
    /// Note, that the contact group is represented by [`Label`]. Currently, this function WON'T
    /// assert if the label has type `ContactGroup`.
    ///
    #[must_use]
    pub fn from_contacts_and_device_contacts(
        contacts: Vec<Contact>,
        contact_groups: Vec<Label>,
        device_contacts: Vec<DeviceContact>,
    ) -> Vec<Self> {
        debug_assert!(contact_groups
            .iter()
            .all(|group| group.label_type == LabelType::ContactGroup));

        // TODO (ET-1971): Extend that implementation
        let (_contacts, _contact_groups, _device_contacts) =
            (contacts, contact_groups, device_contacts);
        vec![]
    }
}

/// Kind of email suggestion
/// Note, variants of this enum are flat - that is, if one contact has assigned two emails,
/// it would be represented by two instances of [`ContactSuggestion`].
///
pub enum ContactSuggestionKind {
    /// Proton contact, stored in the local cache and shared between user devices
    ContactItem(ContactEmailItem),
    /// A device, native contact, stored only locally on the current device.
    DeviceContact(DeviceContactSuggestion),
    /// Proton contact group, that consists only other proton contacts, and never device contact.
    ContactGroup(ContactGroupSuggestion),
}

/// A device, native contact, stored only locally on the current device.
///
pub struct DeviceContactSuggestion {
    /// The field represents the email address used in the device contact
    pub email: String,
}

/// Proton contact group, that consists only other proton contacts, and never device contact.
///
pub struct ContactGroupSuggestion {
    // TODO: I guess that should not be flat?
    pub emails: Vec<ContactEmailItem>,
}
