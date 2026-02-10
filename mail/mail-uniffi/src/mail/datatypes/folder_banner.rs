use proton_mail_common::datatypes::folder_banner::{
    AutoDeleteBanner as RealAutoDeleteBanner, AutoDeleteState as RealAutoDeleteState,
    SpamOrTrash as RealSpamOrTrash,
};
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, uniffi::Record)]
pub struct AutoDeleteBanner {
    pub state: AutoDeleteState,
    pub folder: SpamOrTrash,
}

impl From<RealAutoDeleteBanner> for AutoDeleteBanner {
    fn from(value: RealAutoDeleteBanner) -> Self {
        Self {
            state: value.state.into(),
            folder: value.folder.into(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, uniffi::Enum)]
pub enum AutoDeleteState {
    AutoDeleteUpsell,
    AutoDeleteDisabled,
    AutoDeleteEnabled,
}

impl From<RealAutoDeleteState> for AutoDeleteState {
    fn from(value: RealAutoDeleteState) -> Self {
        match value {
            RealAutoDeleteState::AutoDeleteUpsell => Self::AutoDeleteUpsell,
            RealAutoDeleteState::AutoDeleteDisabled => Self::AutoDeleteDisabled,
            RealAutoDeleteState::AutoDeleteEnabled => Self::AutoDeleteEnabled,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, uniffi::Enum)]
pub enum SpamOrTrash {
    Spam,
    Trash,
}

impl From<RealSpamOrTrash> for SpamOrTrash {
    fn from(value: RealSpamOrTrash) -> Self {
        match value {
            RealSpamOrTrash::Spam => Self::Spam,
            RealSpamOrTrash::Trash => Self::Trash,
        }
    }
}
