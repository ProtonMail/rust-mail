#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub struct AutoDeleteBanner {
    pub state: AutoDeleteState,
    pub folder: SpamOrTrash,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub enum AutoDeleteState {
    AutoDeleteUpsell,
    AutoDeleteDisabled,
    AutoDeleteEnabled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, derive_more::Display)]
pub enum SpamOrTrash {
    #[display("spam")]
    Spam,
    #[display("trash")]
    Trash,
}
