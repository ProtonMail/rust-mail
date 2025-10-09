#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IncludeSwitch {
    #[default]
    Default,

    /// Force scroller to use the `All Mail` label.
    ///
    /// On web, this corresponds to the ...
    ///
    /// > Can't find what you're looking for? Include Spam/Trash.
    ///
    /// ... banner.
    WithSpamAndTrash,
}

impl IncludeSwitch {
    pub fn has_spam_and_trash(&self) -> bool {
        matches!(self, Self::WithSpamAndTrash)
    }
}
