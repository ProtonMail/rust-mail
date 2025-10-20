use crate::core::datatypes::Id;
use crate::errors::ProtonError;
use crate::mail::MailUserSession;
use proton_core_common::datatypes::SystemLabel as RealSystemLabel;
use proton_core_common::models::Label as RealLabel;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use stash::orm::Model;
use std::sync::Arc;
use uniffi::Enum as UniffiEnum;
use uniffi_runtime::uniffi_async;

/// This enum represents the system labels that are available in ProtonMail.
/// Their values corresponds to the remote ids of the labels in the core API database.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, UniffiEnum)]
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

#[uniffi_export]
pub async fn resolve_system_label_id(
    ctx: Arc<MailUserSession>,
    label: SystemLabel,
) -> Result<Option<Id>, ProtonError> {
    let ctx = ctx.ctx()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let local_id = RealSystemLabel::from(label)
            .local_id(&ctx.user_stash().connection().await?)
            .await?
            .map(Into::into);

        Ok(local_id)
    })
    .await
    .map_err(Into::into)
}

#[uniffi_export]
pub async fn resolve_system_label_by_id(
    ctx: Arc<MailUserSession>,
    id: Id,
) -> Result<Option<SystemLabel>, ProtonError> {
    let ctx = ctx.ctx()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let tether = ctx.user_stash().connection().await?;
        let id = id.into();
        let label = RealLabel::load(id, &tether).await?;
        let Some(remote_id) = label.and_then(|l| l.remote_id) else {
            return Ok(None);
        };

        Ok(RealSystemLabel::from_rid(&remote_id).map(Into::into))
    })
    .await
    .map_err(Into::into)
}
