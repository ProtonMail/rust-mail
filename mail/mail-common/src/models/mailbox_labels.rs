use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelExtension;
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, StashError},
};

/// Mailbox labels is an extension over labels, specific for mailbox only.
/// That allows us to keep labels in core-common
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("mailbox_labels")]
pub struct MailboxLabels {
    /// Local id of the label
    #[IdField]
    pub local_label_id: LocalLabelId,

    /// Label has been already initialized by the mailbox, doesn't require additional fetching
    #[DbField]
    pub initialized: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MailboxLabels {
    /// Constructor - note: [`MailboxLabels`] does not implement [`Default`] trait
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            initialized: false,
            row_id: Default::default(),
        }
    }

    /// Save mailbox labels to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to ensure
    /// that if the mailbox label already exists it is updated, and not inserted with a conflict.
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    /// * `tx` - transaction used to modify DB
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.row_id.is_none() {
            if let Some(existing) = Self::find_by_id(self.local_label_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }
        <Self as Model>::save(self, bond).await
    }
}
