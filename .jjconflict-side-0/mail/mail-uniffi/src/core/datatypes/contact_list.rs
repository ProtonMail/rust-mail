use crate::core::datatypes::UnixTimestamp;
use crate::{UniffiEnum, UniffiRecord};
use crate::{core::datatypes::AvatarInformation, core::datatypes::Id};
use itertools::Itertools;
use proton_core_common::datatypes::{
    ContactEmailItem as RealContactEmailItem, ContactGroupItem as RealContactGroupItem,
    ContactItem as RealContactItem, ContactItemType as RealContactItemType,
    ContactSuggestion as RealContactSuggestion, ContactSuggestionKind as RealContactSuggestionKind,
    ContactSuggestions as RealContactSuggestions, DeviceContact as RealDeviceContact,
    DeviceContactSuggestion as RealDeviceContactSuggestion, GroupedContacts as RealGroupedContacts,
};
use proton_core_common::utils::MapVec as _;

/// This is the main data structure that is used to represent the group of contacts.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct GroupedContacts {
    /// The field represent first grapheme of the name of the contact
    pub grouped_by: String,

    // The field represent the list of contacts or groups for the given grapheme
    pub items: Vec<ContactItemType>,
}

impl From<RealGroupedContacts> for GroupedContacts {
    fn from(value: RealGroupedContacts) -> Self {
        Self {
            grouped_by: value.grouped_by,
            items: value.items.map_vec(),
        }
    }
}

/// List of contacts is composed of contacts and groups.
/// This enum is used to represent the either one.
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ContactItemType {
    Contact(ContactItem),
    Group(ContactGroupItem),
}

impl From<RealContactItemType> for ContactItemType {
    fn from(value: RealContactItemType) -> Self {
        match value {
            RealContactItemType::Contact(value) => Self::Contact(value.into()),
            RealContactItemType::Group(value) => Self::Group(value.into()),
        }
    }
}

/// This is the main data structure that is used to represent the contact.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactItem {
    /// The field represent the unique identifier of the contact in the database
    pub id: Id,

    /// The field represent the name of the contact
    pub name: String,

    /// The field represent the avatar information of the contact
    pub avatar_information: AvatarInformation,

    /// The field represent the list of emails of the contact
    pub emails: Vec<ContactEmailItem>,
}

impl From<RealContactItem> for ContactItem {
    fn from(value: RealContactItem) -> Self {
        Self {
            id: value.local_id.into(),
            emails: value.emails.map_vec(),
            avatar_information: value.avatar_information.into(),
            name: value.name,
        }
    }
}

/// This is the main data structure that is used to represent the contact group.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactGroupItem {
    /// The field represent the unique identifier of the contact group in the database
    pub id: Id,

    /// The field represent the name of the contact group
    pub name: String,

    /// The field represent the avatar color of the contact group
    pub avatar_color: String,

    /// The field represent the list of emails of the contact group
    pub contact_emails: Vec<ContactEmailItem>,
}

impl From<RealContactGroupItem> for ContactGroupItem {
    fn from(value: RealContactGroupItem) -> Self {
        Self {
            id: value.local_id.into(),
            contact_emails: value.contacts.map_vec(),
            avatar_color: value.avatar_information.color,
            name: value.name,
        }
    }
}

/// This is the main data structure that is used to represent the contact email.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactEmailItem {
    pub contact_id: Id,
    pub email: String,
    /// The field represents if the email is a proton email like foo@pm.me
    pub is_proton: bool,
    pub last_used_time: UnixTimestamp,
    pub name: String,
    pub avatar_information: AvatarInformation,
}

impl From<RealContactEmailItem> for ContactEmailItem {
    fn from(value: RealContactEmailItem) -> Self {
        Self {
            contact_id: value.local_contact_id.into(),
            email: value.email.into_clear_text_string(),
            is_proton: value.is_proton,
            last_used_time: value.last_used_time.into(),
            avatar_information: value.avatar_information.into(),
            name: value.name,
        }
    }
}

/// Device contact feeded by the mobile/web application.
/// Used as an input for generating list of contact suggestions ([`ContactSuggestion`])
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct DeviceContact {
    /// The field represents unique key identifier used by the user to distinguish elements in the array
    pub key: String,

    /// The field represents the name of the contact
    pub name: String,

    /// List of email addresses assigned to the contact. That list has an arbitrary order given by the user
    pub emails: Vec<String>,
}

impl From<DeviceContact> for RealDeviceContact {
    fn from(value: DeviceContact) -> Self {
        Self {
            key: value.key,
            name: value.name,
            emails: value.emails.map_vec(),
        }
    }
}

/// Collection of sorted contact suggestions
#[derive(uniffi::Object)]
pub struct ContactSuggestions {
    suggestions: RealContactSuggestions,
}

impl From<RealContactSuggestions> for ContactSuggestions {
    fn from(suggestions: RealContactSuggestions) -> Self {
        Self { suggestions }
    }
}

#[uniffi_export]
impl ContactSuggestions {
    /// Returns all contact suggestions
    ///
    #[must_use]
    pub fn all(&self) -> Vec<ContactSuggestion> {
        self.suggestions.all().iter().cloned().map_into().collect()
    }

    /// Returns suggestions filtered by the query
    ///
    #[must_use]
    pub fn filtered(&self, query: &str) -> Vec<ContactSuggestion> {
        self.suggestions
            .filtered(query)
            .into_iter()
            .map_into()
            .collect()
    }
}

/// Used in the composer to suggest email addresses based on the user input (To:, CC: etc fields)
/// Contrary to the [`ContactItemType`] it also might be a device contact
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
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

impl From<RealContactSuggestion> for ContactSuggestion {
    fn from(value: RealContactSuggestion) -> Self {
        Self {
            key: value.key,
            name: value.name,
            avatar_information: value.avatar_information.into(),
            kind: value.kind.into(),
        }
    }
}

/// Kind of email suggestion
/// Note, variants of this enum are flat - that is, if one contact has assigned two emails,
/// it would be represented by two instances of [`ContactSuggestion`].
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ContactSuggestionKind {
    /// Proton contact, stored in the local cache and shared between user devices
    ContactItem(ContactEmailItem),
    /// A device, native contact, stored only locally on the current device.
    DeviceContact(DeviceContactSuggestion),
    /// Proton contact group, that consists only other proton contacts, and never device contact.
    ContactGroup(Vec<ContactEmailItem>),
}

impl From<RealContactSuggestionKind> for ContactSuggestionKind {
    fn from(value: RealContactSuggestionKind) -> Self {
        match value {
            RealContactSuggestionKind::ContactItem(suggestion) => {
                ContactSuggestionKind::ContactItem(suggestion.into())
            }
            RealContactSuggestionKind::DeviceContact(suggestion) => {
                ContactSuggestionKind::DeviceContact(suggestion.into())
            }
            RealContactSuggestionKind::ContactGroup(suggestion) => {
                ContactSuggestionKind::ContactGroup(suggestion.into_iter().map_into().collect())
            }
        }
    }
}

/// A device, native contact, stored only locally on the current device.
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct DeviceContactSuggestion {
    /// The field represents the email address used in the device contact
    pub email: String,
}

impl From<RealDeviceContactSuggestion> for DeviceContactSuggestion {
    fn from(value: RealDeviceContactSuggestion) -> Self {
        Self {
            email: value.email.into_clear_text_string(),
        }
    }
}
