use serde;
use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::stash::Stash;

use crate::utils::{bool_from_integer, bool_to_integer};
use crate::MAX_PAGE_ELEMENT_COUNT;

crate::utils::string_id!(ContactEmailId);
crate::utils::string_id!(ContactId);
crate::utils::string_id!(CardSignature);
crate::utils::string_id!(CardData);
crate::utils::string_id!(ContactLabelId);
crate::utils::string_id!(ContactType);
crate::utils::string_id!(ContactUid);

new_integer_enum!(u8, ContactSendingPreferences {
    Custom = 0,
    Default = 1,
});

new_integer_enum!(u8, CardType {
    ClearText = 0,
    Encrypted = 1,
    Signed = 2,
    EncryptedAndSigned = 3,
});

/// Models the contact email addresses for a contact returned by the API.
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("contact_emails")]
#[allow(clippy::struct_excessive_bools)]
pub struct ContactEmail {
    #[IdField]
    #[serde(rename = "ID")]
    pub id: Option<u64>,
    #[DbField]
    pub remote_id: ContactEmailId,
    #[DbField]
    pub name: String,
    #[DbField]
    pub email: String,
    #[serde(rename = "Type")]
    pub contact_type: Vec<ContactType>,
    #[DbField]
    pub defaults: ContactSendingPreferences,
    #[DbField]
    pub display_order: u32,
    #[DbField]
    #[serde(rename = "ContactID")]
    #[DbField]
    pub contact_id: ContactId,
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<ContactLabelId>,
    #[DbField]
    pub canonical_email: String,
    #[DbField]
    pub last_used_time: u64,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    #[DbField]
    pub is_proton: bool,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

/// Represents a contact card returned by the API.
///
/// Contact cards contain information encoded as a v-card.
/// Cards can be encrypted or signed with the user keys.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct ContactCard {
    #[serde(rename = "Type")]
    pub card_type: CardType,
    pub data: CardData,
    pub signature: Option<CardSignature>,
}

/// A complete contact returned by the API.
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("contacts")]
pub struct Contact {
    #[IdField]
    #[serde(skip)]
    pub id: Option<u64>,
    #[DbField]
    #[serde(rename = "ID")]
    pub remote_id: ContactId,
    #[DbField]
    pub name: String,
    #[DbField]
    #[serde(rename = "UID")]
    pub uid: ContactUid,
    #[DbField]
    pub size: u64,
    #[DbField]
    pub create_time: u64,
    #[DbField]
    pub modify_time: u64,
    pub contact_emails: Vec<ContactEmail>,
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<ContactLabelId>,
    pub cards: Vec<ContactCard>,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

/// Parameters to filter/search contacts with a given criteria on API requests.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ContactFilter {
    /// Email to filter on. Only relevant when searching contact emails.
    pub email: Option<String>,
    /// Label id to filter on.
    pub label_id: Option<ContactLabelId>,
    /// Page index
    pub page: u64,
    /// Number of elements per page.
    pub page_size: u64,
}

impl Default for ContactFilter {
    fn default() -> Self {
        Self {
            email: None,
            label_id: None,
            page: 0,
            page_size: MAX_PAGE_ELEMENT_COUNT as u64,
        }
    }
}

impl ContactFilter {
    fn new(page_index: usize, page_size: usize) -> Self {
        Self {
            page: page_index as u64,
            page_size: page_size.max(MAX_PAGE_ELEMENT_COUNT) as u64,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn new_builder(page_index: usize, page_size: usize) -> ContactFilterBuilder {
        ContactFilterBuilder::new(page_index, page_size)
    }
}

/// Builder to create a [`ContactFilter`].
#[derive(Debug, Default)]
pub struct ContactFilterBuilder(ContactFilter);

impl ContactFilterBuilder {
    /// Creates new [`ContactFilterBuilder`].
    #[must_use]
    pub fn new(page_index: usize, page_size: usize) -> Self {
        Self(ContactFilter::new(page_index, page_size))
    }

    /// Filters the contacts by e-mail address.
    #[must_use]
    pub fn with_email(mut self, email_address: String) -> ContactFilterBuilder {
        self.0.email = Some(email_address);
        self
    }

    /// Filters the contacts by label identifier.
    #[must_use]
    pub fn with_label_id(mut self, label_id: ContactLabelId) -> ContactFilterBuilder {
        self.0.label_id = Some(label_id);
        self
    }

    /// Creates a new [`ContactFilter`] from the given builder.
    #[must_use]
    pub fn build(self) -> ContactFilter {
        self.0
    }
}
