use crate::datatypes::{
    ContactSendingPreferences, ContactTypes, Id, LabelId, Labels, LocalId, RemoteId,
};
use crate::models::{Contact, ModelExtension};
use proton_api_core::services::proton::response_data::ContactEmail as ApiContactEmail;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_emails")]
pub struct ContactEmail {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.

    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_contact_id: Option<RemoteId>,

    #[DbField]
    pub local_contact_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub canonical_email: String,

    /// TODO: Document this field.
    #[DbField]
    pub contact_type: ContactTypes,

    /// TODO: Document this field.
    #[DbField]
    pub defaults: ContactSendingPreferences,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub is_proton: bool,

    /// TODO: Document this field.
    #[DbField]
    pub label_ids: Labels,

    /// TODO: Document this field.
    #[DbField]
    pub last_used_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub name: String,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiContactEmail> for ContactEmail {
    fn from(value: ApiContactEmail) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
            local_contact_id: None,
            remote_contact_id: Some(value.contact_id.into()),
            canonical_email: value.canonical_email,
            contact_type: ContactTypes::new(value.contact_type),
            defaults: value.defaults.into(),
            display_order: value.order,
            email: value.email,
            is_proton: value.is_proton,
            label_ids: Labels::new(value.label_ids.into_iter().map(LabelId::from).collect()),
            last_used_time: value.last_used_time,
            name: value.name,
            row_id: None,
            stash: None,
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
            stash: Default::default(),
        }
    }
}

impl ContactEmail {
    /// Save a contact email to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a contact email to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
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
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
            }
        }

        if let Some(contact_remote_id) = self.remote_contact_id.clone() {
            self.local_contact_id = contact_remote_id
                .counterpart::<Contact, _>(interface)
                .await?;
        }

        <Self as Model>::save_using(self, interface).await
    }
}
