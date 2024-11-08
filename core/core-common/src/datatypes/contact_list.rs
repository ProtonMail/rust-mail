use super::avatar::AvatarInformation;
use crate::{
    datatypes::LocalId,
    models::{Contact, ContactEmail},
};
use itertools::Itertools;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

const DEFAULT_GROUP: &str = "#";

/// This is the main data structure that is used to represent the group of contacts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupedContacts {
    /// The field represent first grapheme of the name of the contact
    pub grouped_by: String,

    // The field represent the list of contacts or groups for the given grapheme
    pub item: Vec<ContactItemType>,
}

impl GroupedContacts {
    pub fn from_contacts(value: Vec<Contact>) -> Vec<Self> {
        let mut btmap: BTreeMap<String, Vec<ContactItemType>> = BTreeMap::new();

        value
            .into_iter()
            .map(ContactItem::from)
            .sorted_by(|one, other| {
                let one_words: String = one.name.unicode_words().collect();
                let other_words: String = other.name.unicode_words().collect();
                one_words.cmp(&other_words)
            })
            .for_each(|contact| {
                let key = contact.avatar_information.text.clone();
                let key = if key.is_empty() || key.as_str() == "?" {
                    DEFAULT_GROUP.to_string()
                } else {
                    key
                };

                btmap.entry(key).or_default().push(contact.into());
            });

        btmap
            .into_iter()
            .map(|(grouped_by, item)| GroupedContacts { grouped_by, item })
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
    pub local_id: LocalId,

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
            emails: value.contact_emails.into_iter().map(Into::into).collect(),
            name: value.name,
        }
    }
}

/// This is the main data structure that is used to represent the contact group.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactGroupItem {
    /// The field represent the unique identifier of the contact group in the database
    pub local_id: LocalId,

    /// The field represent the name of the contact group
    pub name: String,

    /// The field represent the avatar color of the contact group
    pub avatar_color: String,

    /// The field represent the list of emails of the contact group
    pub emails: Vec<ContactEmailItem>,
}

/// This is the main data structure that is used to represent the contact email.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactEmailItem {
    /// The field represent the unique identifier of the contact email in the database
    pub local_id: LocalId,

    /// The field represent the email of the contact
    pub email: String,
}

impl From<ContactEmail> for ContactEmailItem {
    fn from(value: ContactEmail) -> Self {
        Self {
            local_id: value.local_id.unwrap(),
            email: value.email,
        }
    }
}
