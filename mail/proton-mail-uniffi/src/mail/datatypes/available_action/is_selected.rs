use crate::UniffiEnum;

#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum IsSelected {
    Selected,
    Unselected,
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
