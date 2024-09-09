use crate::UniffiEnum;

/// This enum represents the state of a selection.
/// Actions such as MoveAction or LabelAsAction should
/// be able to represent the state of its selection.
///
#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum IsSelected {
    /// All actions on any number of items are selected or not applied.
    Selected,

    /// All actions on any number of items are unselected (= not applied).
    Unselected,

    /// Some actions on any number of items are selected, some are unselected.
    Partial,
}

impl IsSelected {
    #[must_use]
    pub fn new(selected: Option<bool>) -> Self {
        match selected {
            Some(true) => IsSelected::Selected,
            Some(false) => IsSelected::Unselected,
            None => IsSelected::Partial,
        }
    }
}
