use proton_mail_common::datatypes::MovableSystemFolder as RealMovableSystemFolder;
use uniffi::Enum as UniffiEnum;

/// This enum represents the system labels that are valid target for Move actions.
/// Their values correspond to the remote ids of the labels in the core API database.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, UniffiEnum)]
#[repr(u8)]
pub enum MovableSystemFolder {
    Inbox = 0,
    Trash = 3,
    Spam = 4,
    Archive = 6,
}

impl From<RealMovableSystemFolder> for MovableSystemFolder {
    fn from(label: RealMovableSystemFolder) -> Self {
        match label {
            RealMovableSystemFolder::Inbox => Self::Inbox,
            RealMovableSystemFolder::Trash => Self::Trash,
            RealMovableSystemFolder::Spam => Self::Spam,
            RealMovableSystemFolder::Archive => Self::Archive,
        }
    }
}

impl From<MovableSystemFolder> for RealMovableSystemFolder {
    fn from(label: MovableSystemFolder) -> Self {
        match label {
            MovableSystemFolder::Inbox => Self::Inbox,
            MovableSystemFolder::Trash => Self::Trash,
            MovableSystemFolder::Spam => Self::Spam,
            MovableSystemFolder::Archive => Self::Archive,
        }
    }
}
