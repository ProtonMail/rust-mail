#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AutoDeleteBanner {
    pub state: AutoDeleteState,
    pub folder: SpamOrTrash,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AutoDeleteState {
    AutoDeleteUpsell,
    AutoDeleteDisabled,
    AutoDeleteEnabled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, derive_more::Display)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SpamOrTrash {
    #[display("spam")]
    Spam,
    #[display("trash")]
    Trash,
}
