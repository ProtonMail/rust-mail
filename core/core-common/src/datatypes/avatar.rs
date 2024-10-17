#[cfg(test)]
#[path = "../tests/datatypes/avatar.rs"]
mod tests;

use crate::utils::{first_grapheme_upppercase, proton_color};
use unicode_segmentation::UnicodeSegmentation;

/// This is the main data structure that is used to represent the avatar information.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct AvatarInformation {
    /// The field represent the first two grapheme (if available) of the name of the contact
    /// those could be viewed as initials of the contact.
    pub text: String,

    /// The field represent the color of the avatar.
    pub color: String,
}

impl AvatarInformation {
    /// Returns true if the text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Returns a new `AvatarInformation` with the given value if the text is empty.
    ///
    /// # Parameters
    ///
    /// * `value` - The value to use if the text is empty.
    ///
    #[must_use]
    pub fn or_else<I>(self, value: I) -> Self
    where
        I: Into<Self>,
    {
        if self.is_empty() {
            value.into()
        } else {
            self
        }
    }
}

impl<S> From<S> for AvatarInformation
where
    S: AsRef<str>,
{
    fn from(value: S) -> Self {
        let name = value.as_ref();
        let is_email = name.contains('@');
        let text = name
            .unicode_words()
            .take(if is_email { 1 } else { 2 })
            .filter_map(first_grapheme_upppercase)
            .collect();
        let color = proton_color(name);

        Self {
            text,
            color: color.to_string(),
        }
        .or_else(Self {
            text: first_grapheme_upppercase(name).unwrap_or_default(),
            color: color.to_string(),
        })
    }
}
