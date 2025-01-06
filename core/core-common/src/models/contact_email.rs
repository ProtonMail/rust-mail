use crate::datatypes::{
    ContactSendingPreferences, ContactTypes, IdCounterpart, Labels, LocalContactEmailId,
    LocalContactId,
};
use crate::models::{Contact, ModelExtension};
use proton_api_core::services::proton::common::{ContactEmailId, ContactId};
use proton_api_core::services::proton::response_data::ContactEmail as ApiContactEmail;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError};

/// Represents a contact's email.
///
/// Contact emails are used to store email addresses associated with a contact.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_emails")]
pub struct ContactEmail {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalContactEmailId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<ContactEmailId>,

    /// Remote contact ID to which this email belongs.
    #[DbField]
    pub remote_contact_id: Option<ContactId>,

    /// Local contact ID to which this email belongs.
    #[DbField]
    pub local_contact_id: Option<LocalContactId>,

    /// Canonical email address.
    #[DbField]
    pub canonical_email: String,

    /// Contact type, free text label of the contact.
    #[DbField]
    pub contact_type: ContactTypes,

    /// Contact sending preferences: 0 - custom, 1 - default.
    #[DbField]
    pub defaults: ContactSendingPreferences,

    /// Display order of the email (based on creation time).
    #[DbField]
    pub display_order: u32,

    /// Email address.
    #[DbField]
    pub email: String,

    /// Indicates if the email is a Proton email.
    #[DbField]
    pub is_proton: bool,

    /// Label IDs associated with the email. Label IDs are used to group emails.
    #[DbField]
    pub label_ids: Labels,

    /// Last used time of the email.
    #[DbField]
    pub last_used_time: u64,

    /// Name of the email.
    #[DbField]
    pub name: String,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl From<ApiContactEmail> for ContactEmail {
    fn from(value: ApiContactEmail) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            local_contact_id: None,
            remote_contact_id: Some(value.contact_id),
            canonical_email: value.canonical_email,
            contact_type: ContactTypes::new(value.contact_type),
            defaults: value.defaults.into(),
            display_order: value.order,
            email: value.email,
            is_proton: value.is_proton,
            label_ids: Labels::new(value.label_ids),
            last_used_time: value.last_used_time,
            name: value.name,
            row_id: None,
        }
    }
}

#[cfg(any(test, debug_assertions))]
impl Default for ContactEmail {
    #[allow(clippy::default_trait_access)]
    fn default() -> Self {
        Self {
            local_id: Default::default(),
            remote_id: Default::default(),
            remote_contact_id: Default::default(),
            local_contact_id: Default::default(),
            canonical_email: Default::default(),
            contact_type: Default::default(),
            defaults: ContactSendingPreferences::Default,
            display_order: Default::default(),
            email: Default::default(),
            is_proton: Default::default(),
            label_ids: Default::default(),
            last_used_time: Default::default(),
            name: Default::default(),
            row_id: Default::default(),
        }
    }
}

impl ContactEmail {
    /// Save a contact email to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, bond).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
            }
        }

        if let Some(contact_remote_id) = self.remote_contact_id.clone() {
            self.local_contact_id = contact_remote_id.counterpart::<Contact>(bond).await?;
        }

        <Self as Model>::save(self, bond).await
    }
}
