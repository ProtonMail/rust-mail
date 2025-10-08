#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IncludeFilter {
    #[default]
    Default,

    /// Force scroller to use the `Almost All Mail` label.
    ///
    /// On web, this corresponds to the ...
    ///
    /// > Can't find what you're looking for? Include Spam/Trash.
    ///
    /// ... banner.
    WithSpamAndTrash,
}

impl IncludeFilter {
    pub fn has_spam_and_trash(&self) -> bool {
        matches!(self, Self::WithSpamAndTrash)
    }
}
