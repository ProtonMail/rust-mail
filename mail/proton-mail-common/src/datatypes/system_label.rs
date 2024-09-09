use std::fmt::Display;

use crate::{
    datatypes::{LabelId, LabelType},
    models::Label,
};

/// This enum represents the system labels that are available in ProtonMail.
/// Their values corresponds to the remote ids of the labels in the core API database.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
    AlmostAllMail = 15,
    Snoozed = 16,
    CategorySocial = 20,
    CategoryPromotions = 21,
    CatergoryUpdates = 22,
    CategoryForums = 23,
    CategoryDefault = 24,
}

impl SystemLabel {
    pub fn new(label: &Label) -> Option<Self> {
        match label.label_type {
            LabelType::Label | LabelType::ContactGroup | LabelType::Folder => None,
            LabelType::System => Self::from_rid(label.remote_id.as_ref()),
        }
    }

    pub fn from_rid(label_id: Option<&LabelId>) -> Option<Self> {
        let remote_id = label_id?.parse::<u8>().ok()?;

        match remote_id {
            x if x == Self::Inbox as u8 => Some(Self::Inbox),
            x if x == Self::AllDrafts as u8 => Some(Self::AllDrafts),
            x if x == Self::AllSent as u8 => Some(Self::AllSent),
            x if x == Self::Trash as u8 => Some(Self::Trash),
            x if x == Self::Spam as u8 => Some(Self::Spam),
            x if x == Self::AllMail as u8 => Some(Self::AllMail),
            x if x == Self::Archive as u8 => Some(Self::Archive),
            x if x == Self::Sent as u8 => Some(Self::Sent),
            x if x == Self::Drafts as u8 => Some(Self::Drafts),
            x if x == Self::Outbox as u8 => Some(Self::Outbox),
            x if x == Self::Starred as u8 => Some(Self::Starred),
            x if x == Self::Scheduled as u8 => Some(Self::Scheduled),
            x if x == Self::AlmostAllMail as u8 => Some(Self::AlmostAllMail),
            x if x == Self::Snoozed as u8 => Some(Self::Snoozed),
            x if x == Self::CategorySocial as u8 => Some(Self::CategorySocial),
            x if x == Self::CategoryPromotions as u8 => Some(Self::CategoryPromotions),
            x if x == Self::CatergoryUpdates as u8 => Some(Self::CatergoryUpdates),
            x if x == Self::CategoryForums as u8 => Some(Self::CategoryForums),
            x if x == Self::CategoryDefault as u8 => Some(Self::CategoryDefault),
            _ => None,
        }
    }

    pub fn is_exclusive_location(&self) -> bool {
        matches!(
            self,
            Self::Inbox
                | Self::Trash
                | Self::Archive
                | Self::Spam
                | Self::Snoozed
                | Self::Scheduled
                | Self::Outbox
        )
    }

    pub fn exclusive_locations() -> [Self; 7] {
        [
            Self::Inbox,
            Self::Trash,
            Self::Archive,
            Self::Spam,
            Self::Snoozed,
            Self::Scheduled,
            Self::Outbox,
        ]
    }

    pub fn is_movable_folder(&self) -> bool {
        matches!(self, Self::Inbox | Self::Trash | Self::Archive | Self::Spam)
    }

    pub fn movable_folders() -> [Self; 4] {
        [Self::Inbox, Self::Trash, Self::Archive, Self::Spam]
    }

    pub fn is_starred(&self) -> bool {
        *self == Self::Starred
    }

    pub fn label_id(&self) -> LabelId {
        LabelId::from(self.to_string())
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
