use crate::datatypes::{
    ContactSendingPreferences, ContactTypes, Labels, LocalContactEmailId, LocalContactId,
    UnixTimestamp,
};
use crate::models::{Contact, Label, ModelIdExtension};
use proton_core_api::services::proton::{ContactEmail as ApiContactEmail, PrivateEmail};
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
    #[IdField(autoincrement)]
    pub local_id: Option<LocalContactEmailId>,

    #[DbField]
    pub remote_id: Option<ContactEmailId>,

    #[DbField]
    pub remote_contact_id: Option<ContactId>,

    // The seeming optionality in this field exists only for syncing with the API, to make
    // it less awkward, in theory it could be removed.
    // This is always safe to unwrap except when converting this from an API type.
    #[DbField]
    pub local_contact_id: Option<LocalContactId>,

    #[DbField]
    pub canonical_email: PrivateEmail,

    #[DbField]
    pub contact_type: ContactTypes,

    #[DbField]
    pub defaults: ContactSendingPreferences,

    #[DbField]
    pub display_order: u32,

    #[DbField]
    pub email: PrivateEmail,

    #[DbField]
    pub is_proton: bool,

    #[DbField]
    pub label_ids: Labels,

    #[DbField]
    pub last_used_time: UnixTimestamp,

    #[DbField]
    pub name: String,
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
            last_used_time: value.last_used_time.into(),
            name: value.name,
        }
    }
}

impl ContactEmail {
    #[cfg(feature = "test-utils")]
    #[allow(clippy::default_trait_access)]
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            local_contact_id: Some(0.into()),
            local_id: Default::default(),
            remote_id: Default::default(),
            remote_contact_id: Default::default(),
            canonical_email: Default::default(),
            contact_type: Default::default(),
            defaults: ContactSendingPreferences::Default,
            display_order: Default::default(),
            email: Default::default(),
            is_proton: Default::default(),
            label_ids: Default::default(),
            last_used_time: UnixTimestamp::new(0),
            name: Default::default(),
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
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone()
            && let Some(existing) = Self::find_by_remote_id(remote_id, bond).await?
        {
            self.local_id = existing.local_id;
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
