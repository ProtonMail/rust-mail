use crate::{core::datatypes::AvatarInformation, core::datatypes::Id};
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::datatypes::{
    ContactEmailItem as RealContactEmailItem, ContactGroupItem as RealContactGroupItem,
    ContactItem as RealContactItem, ContactItemType as RealContactItemType,
    GroupedContacts as RealGroupedContacts,
};

/// This is the main data structure that is used to represent the group of contacts.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct GroupedContacts {
    /// The field represent first grapheme of the name of the contact
    pub grouped_by: String,

    // The field represent the list of contacts or groups for the given grapheme
    pub item: Vec<ContactItemType>,
}

impl From<RealGroupedContacts> for GroupedContacts {
    fn from(value: RealGroupedContacts) -> Self {
        Self {
            grouped_by: value.grouped_by,
            item: value.item.into_iter().map(Into::into).collect(),
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
            emails: value.emails.into_iter().map(Into::into).collect(),
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
    pub emails: Vec<ContactEmailItem>,
}

impl From<RealContactGroupItem> for ContactGroupItem {
    fn from(value: RealContactGroupItem) -> Self {
        Self {
            id: value.local_id.into(),
            emails: value.emails.into_iter().map(Into::into).collect(),
            avatar_color: value.avatar_color,
            name: value.name,
        }
    }
}

/// This is the main data structure that is used to represent the contact email.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactEmailItem {
    /// The field represent the unique identifier of the contact email in the database
    pub id: Id,

    /// The field represent the email of the contact
    pub email: String,
}

impl From<RealContactEmailItem> for ContactEmailItem {
    fn from(value: RealContactEmailItem) -> Self {
        Self {
            id: value.local_id.into(),
            email: value.email,
        }
    }
}
