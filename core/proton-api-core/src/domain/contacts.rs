use serde;
use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether};
use stash::{params, sql_using_serde};
use tracing::error;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Labels(pub Vec<ContactLabelId>);

sql_using_serde!(Labels);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContactTypes(pub Vec<ContactType>);

sql_using_serde!(ContactTypes);

/// Models the contact email addresses for a contact returned by the API.
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("contact_emails")]
#[allow(clippy::struct_excessive_bools)]
pub struct ContactEmail {
    #[IdField(optional)]
    #[serde(rename = "ID")]
    pub remote_id: Option<ContactEmailId>,
    #[DbField]
    pub name: String,
    #[DbField]
    pub email: String,
    #[DbField]
    #[serde(rename = "Type")]
    pub contact_type: ContactTypes,
    #[DbField]
    pub defaults: ContactSendingPreferences,
    #[DbField]
    pub display_order: u32,
    #[DbField]
    #[serde(rename = "ContactID")]
    pub remote_contact_id: Option<ContactId>,
    #[DbField]
    #[serde(rename = "LabelIDs")]
    pub label_ids: Labels,
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
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("contact_cards")]
pub struct ContactCard {
    #[IdField(autoincrement)]
    #[serde(skip)]
    pub id: Option<u64>,
    #[DbField]
    #[serde(skip)]
    pub remote_contact_id: Option<ContactId>,
    #[DbField]
    #[serde(rename = "Type")]
    pub card_type: CardType,
    #[DbField]
    pub data: CardData,
    #[DbField]
    pub signature: Option<CardSignature>,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

/// A complete contact returned by the API.
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("contacts")]
pub struct Contact {
    #[IdField(optional)]
    #[serde(rename = "ID")]
    pub remote_id: Option<ContactId>,
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
    #[DbField]
    #[serde(rename = "LabelIDs")]
    pub label_ids: Labels,
    pub cards: Vec<ContactCard>,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

impl Contact {
    /// Returns the associated cards for a contact.
    ///
    /// This function retrieves the cards for a contact, stores them in the
    /// contact struct, and then returns them.
    ///
    /// # Errors
    ///
    /// Returns a [`StashError`] if the cards cannot be retrieved.
    ///
    pub async fn cards(&mut self) -> Result<&Vec<ContactCard>, StashError> {
        let Some(stash) = self.stash() else {
            return Err(StashError::NoStashAvailable);
        };
        self.cards = ContactCard::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            stash,
            None,
        )
        .await?;
        Ok(&self.cards)
    }

    /// Returns the associated emails for a contact.
    ///
    /// This function retrieves the emails for a contact, stores them in the
    /// contact struct, and then returns them.
    ///
    /// # Errors
    ///
    /// Returns a [`StashError`] if the emails cannot be retrieved.
    ///
    pub async fn emails(&mut self) -> Result<&Vec<ContactEmail>, StashError> {
        let Some(stash) = self.stash() else {
            return Err(StashError::NoStashAvailable);
        };
        self.contact_emails = ContactEmail::find(
            "WHERE remote_contact_id = ?",
            params![self.remote_id.clone()],
            stash,
            None,
        )
        .await?;
        Ok(&self.contact_emails)
    }

    /// Overrides [`Model::save()`] to set the contact id for children.
    pub async fn save(&mut self) -> Result<(), StashError> {
        Model::save(self).await?;
        for card in &mut self.cards {
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        let Some(stash) = self.stash() else {
            return Err(StashError::NoStashAvailable);
        };
        stash
            .execute(
                "DELETE FROM contact_cards WHERE remote_contact_id = ?",
                params![self.remote_id.clone()],
            )
            .await?;
        for card in &mut self.cards {
            card.id = None;
            card.row_id = None;
            card.save().await.map_err(|e| {
                error!("Failed to update contact cards: {e}");
                e
            })?;
        }
        Ok(())
    }

    /// Overrides [`Model::save_using()`] to set the contact id for children.
    pub async fn save_using(&mut self, tether: &Tether) -> Result<(), StashError> {
        Model::save_using(self, tether).await?;
        for card in &mut self.cards {
            card.remote_contact_id.clone_from(&self.remote_id);
        }
        for email in &mut self.contact_emails {
            email.remote_contact_id.clone_from(&self.remote_id);
        }
        tether
            .execute(
                "DELETE FROM contact_cards WHERE remote_contact_id = ?",
                params![self.remote_id.clone()],
            )
            .await?;
        for card in &mut self.cards {
            card.id = None;
            card.row_id = None;
            card.save_using(&tether).await.map_err(|e| {
                error!("Failed to update contact cards: {e}");
                e
            })?;
        }
        Ok(())
    }
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
