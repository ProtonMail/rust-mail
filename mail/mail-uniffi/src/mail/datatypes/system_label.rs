use proton_core_common::datatypes::SystemLabel as RealSystemLabel;
use proton_core_common::models::Label as RealLabel;
use uniffi::Enum as UniffiEnum;

/// This enum represents the system labels that are available in ProtonMail.
/// Their values corresponds to the remote ids of the labels in the core API database.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, UniffiEnum)]
pub enum SystemLabel {
    Inbox,
    AllDrafts,
    AllSent,
    Trash,
    Spam,
    AllMail,
    Archive,
    Sent,
    Drafts,
    Outbox,

    Starred,
    Scheduled,
    Blocked,
    AlmostAllMail,
    Snoozed,
    Pinned,

    CategorySocial,
    CategoryPromotions,
    CatergoryUpdates,
    CategoryForums,
    CategoryDefault,
}

impl SystemLabel {
    pub fn new(rl: &RealLabel) -> Option<Self> {
        RealSystemLabel::new(rl).map(Into::into)
    }
}

impl From<RealSystemLabel> for SystemLabel {
    fn from(label: RealSystemLabel) -> Self {
        match label {
            RealSystemLabel::Inbox => SystemLabel::Inbox,
            RealSystemLabel::AllDrafts => SystemLabel::AllDrafts,
            RealSystemLabel::AllSent => SystemLabel::AllSent,
            RealSystemLabel::Trash => SystemLabel::Trash,
            RealSystemLabel::Spam => SystemLabel::Spam,
            RealSystemLabel::AllMail => SystemLabel::AllMail,
            RealSystemLabel::Archive => SystemLabel::Archive,
            RealSystemLabel::Sent => SystemLabel::Sent,
            RealSystemLabel::Drafts => SystemLabel::Drafts,
            RealSystemLabel::Outbox => SystemLabel::Outbox,
            RealSystemLabel::Starred => SystemLabel::Starred,
            RealSystemLabel::Scheduled => SystemLabel::Scheduled,
            RealSystemLabel::AlmostAllMail => SystemLabel::AlmostAllMail,
            RealSystemLabel::Snoozed => SystemLabel::Snoozed,
            RealSystemLabel::CategorySocial => SystemLabel::CategorySocial,
            RealSystemLabel::CategoryPromotions => SystemLabel::CategoryPromotions,
            RealSystemLabel::CatergoryUpdates => SystemLabel::CatergoryUpdates,
            RealSystemLabel::CategoryForums => SystemLabel::CategoryForums,
            RealSystemLabel::CategoryDefault => SystemLabel::CategoryDefault,
            RealSystemLabel::Blocked => SystemLabel::Blocked,
            RealSystemLabel::Pinned => SystemLabel::Pinned,
        }
    }
}

impl From<SystemLabel> for RealSystemLabel {
    fn from(label: SystemLabel) -> Self {
        match label {
            SystemLabel::Inbox => RealSystemLabel::Inbox,
            SystemLabel::AllDrafts => RealSystemLabel::AllDrafts,
            SystemLabel::AllSent => RealSystemLabel::AllSent,
            SystemLabel::Trash => RealSystemLabel::Trash,
            SystemLabel::Spam => RealSystemLabel::Spam,
            SystemLabel::AllMail => RealSystemLabel::AllMail,
            SystemLabel::Archive => RealSystemLabel::Archive,
            SystemLabel::Sent => RealSystemLabel::Sent,
            SystemLabel::Drafts => RealSystemLabel::Drafts,
            SystemLabel::Outbox => RealSystemLabel::Outbox,
            SystemLabel::Starred => RealSystemLabel::Starred,
            SystemLabel::Scheduled => RealSystemLabel::Scheduled,
            SystemLabel::AlmostAllMail => RealSystemLabel::AlmostAllMail,
            SystemLabel::Snoozed => RealSystemLabel::Snoozed,
            SystemLabel::CategorySocial => RealSystemLabel::CategorySocial,
            SystemLabel::CategoryPromotions => RealSystemLabel::CategoryPromotions,
            SystemLabel::CatergoryUpdates => RealSystemLabel::CatergoryUpdates,
            SystemLabel::CategoryForums => RealSystemLabel::CategoryForums,
            SystemLabel::CategoryDefault => RealSystemLabel::CategoryDefault,
            SystemLabel::Blocked => RealSystemLabel::Blocked,
            SystemLabel::Pinned => RealSystemLabel::Pinned,
        }
    }
}
