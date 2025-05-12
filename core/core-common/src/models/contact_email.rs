use crate::datatypes::{
    ContactSendingPreferences, ContactTypes, Labels, LocalContactEmailId, LocalContactId,
};
use crate::models::{Contact, Label, ModelIdExtension};
use proton_core_api::services::proton::ContactEmail as ApiContactEmail;
use proton_core_api::services::proton::{ContactEmailId, ContactId, LabelId};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError, Tether};

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

impl ModelIdExtension for ContactEmail {
    type RemoteId = ContactEmailId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
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
    ///   use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_remote_id(remote_id, bond).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
            }
        }

        if let Some(contact_remote_id) = self.remote_contact_id.clone() {
            self.local_contact_id = Contact::remote_id_counterpart(contact_remote_id, bond).await?;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Count the number of emails in a contact group with name `group_name`.
    ///
    /// If the group could not be found, this method returns `None`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn count_in_contact_group_by_name(
        group_name: String,
        tether: &Tether,
    ) -> Result<Option<usize>, StashError> {
        // Resolve label
        let Some(label) = Label::find_first("WHERE name = ?", params![group_name], tether).await?
        else {
            return Ok(None);
        };

        // Contact emails are not stored with local id information at the moment.
        // TODO(post-release): Should be using local ids instead of remote.
        // This is not a problem at the moment since we can not create contact groups.
        let Some(remote_id) = label.remote_id else {
            return Ok(None);
        };

        Self::count_in_contact_group(remote_id, tether)
            .await
            .map(Some)
    }

    /// Count the number of emails in a contact group with `contact_group_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn count_in_contact_group(
        contact_group_id: LabelId,
        tether: &Tether,
    ) -> Result<usize, StashError> {
        // Unfortunately, at this time the ids are not stored in relational table, so we need
        // to decode the raw json.
        // TODO(post-release): Transform into relational table.
        tether.query_value::<_, usize>(format!(
            "SELECT DISTINCT COUNT(local_id) AS value FROM {}, json_each({}.label_ids) WHERE json_each.value = ?",
            Self::table_name(),
            Self::table_name()
        ), params![contact_group_id]).await
    }
}
