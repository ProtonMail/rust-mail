use std::fmt::Display;

use crate::datatypes::LocalLabelId;
use crate::models::{Label, ModelIdExtension};
use derive_more::TryFrom;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{StashError, Tether};

use crate::datatypes::{LabelId, LabelType};

/// This enum represents the system labels that are available in `ProtonMail`.
/// Their values correspond to the remote ids of the labels in the core API database.
///
/// # Agnostic nature
///
/// Note, that even though the [`SystemLabel`] is in `core_common` crate, it is not fully
/// agnostic. `Spam`, `AllSent` or `AlmostAllMail` are not usable outside of the Mail context.
///
/// However, the main reason why this enum exist, is to ensure that all default system labels are present in
/// local database. In that case we are less interested into the purpose of those labels and more in
/// knowing that are built-in, predefined labels.
///
/// In the future this enum might be extended by labels from other contexts
#[derive(
    Copy, Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, TryFrom,
)]
#[try_from(repr)]
#[repr(u8)]
pub enum SystemLabel {
    Inbox = 0,
    AllDrafts = 1,
    AllSent = 2,
    Trash = 3,
    Spam = 4,
    AllMail = 5,
    Archive = 6,
    Sent = 7,
    Drafts = 8,
    Outbox = 9,

    Starred = 10,
    Scheduled = 12,
    Blocked = 14,
    AlmostAllMail = 15,
    Snoozed = 16,
    Pinned = 17,

    CategorySocial = 20,
    CategoryPromotions = 21,
    CatergoryUpdates = 22,
    CategoryForums = 23,
    CategoryDefault = 24,
}

impl SystemLabel {
    #[must_use]
    pub fn new(label: &Label) -> Option<Self> {
        match label.label_type {
            LabelType::Label | LabelType::ContactGroup | LabelType::Folder => None,
            LabelType::System => Self::from_opt_rid(label.remote_id.as_ref()),
        }
    }

    #[must_use]
    pub fn from_opt_rid(label_id: Option<&LabelId>) -> Option<Self> {
        let remote_id = label_id?.parse::<u8>().ok()?;
        Self::try_from(remote_id).ok()
    }

    #[must_use]
    pub fn from_rid(label_id: &LabelId) -> Option<Self> {
        Self::from_opt_rid(Some(label_id))
    }

    #[must_use]
    pub fn is_exclusive_location(&self) -> bool {
        Self::exclusive_locations().contains(self)
    }

    #[must_use]
    pub fn exclusive_locations() -> [Self; 9] {
        [
            Self::Inbox,
            Self::Trash,
            Self::Archive,
            Self::Spam,
            Self::Snoozed,
            Self::Scheduled,
            Self::Outbox,
            Self::Drafts,
            Self::Sent,
        ]
    }

    #[must_use]
    pub fn is_movable_folder(&self) -> bool {
        matches!(self, Self::Inbox | Self::Trash | Self::Archive | Self::Spam)
    }

    #[must_use]
    pub fn movable_folders() -> [Self; 4] {
        [Self::Inbox, Self::Trash, Self::Archive, Self::Spam]
    }

    #[must_use]
    pub fn is_starred(&self) -> bool {
        *self == Self::Starred
    }

    #[must_use]
    pub fn is_snoozed(&self) -> bool {
        *self == Self::Snoozed
    }

    #[must_use]
    pub fn is_snooze_location(&self) -> bool {
        matches!(self, Self::Snoozed | Self::Inbox)
    }

    #[must_use]
    pub fn label_id(&self) -> LabelId {
        LabelId::from(self.to_string())
    }

    #[must_use]
    pub fn remote_id(&self) -> LabelId {
        self.label_id()
    }

    pub async fn local_id(&self, tether: &Tether) -> Result<Option<LocalLabelId>, StashError> {
        Label::remote_id_counterpart(self.remote_id(), tether).await
    }

    pub async fn load(&self, tether: &Tether) -> Result<Option<Label>, StashError> {
        let Some(local_id) = self.local_id(tether).await? else {
            return Ok(None);
        };

        Label::load(local_id, tether).await
    }
}

impl Display for SystemLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

impl From<SystemLabel> for LabelId {
    fn from(label: SystemLabel) -> Self {
        LabelId::from(label.to_string())
    }
}
