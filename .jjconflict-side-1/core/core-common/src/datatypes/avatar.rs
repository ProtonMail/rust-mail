#[cfg(test)]
#[path = "../tests/datatypes/avatar.rs"]
mod tests;

use crate::utils::{first_grapheme_upppercase, proton_color};
use unicode_segmentation::UnicodeSegmentation;

/// This is the main data structure that is used to represent the avatar information.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AvatarInformation {
    /// The field represent the first grapheme of the name of the contact
    pub text: String,

    /// The field represent the color of the avatar.
    pub color: String,
}

/// Default avatar information if there is no recipient e.g. in draft.
impl Default for AvatarInformation {
    fn default() -> Self {
        AvatarInformation {
            text: "?".to_string(),
            color: "#A7AAB0".to_string(),
        }
    }
}

impl AvatarInformation {
    /// Returns true if the text is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Returns a new `AvatarInformation` with the given value if the text is empty.
    ///
    #[must_use]
    pub fn or_else<I>(self, value: I) -> Self
    where
        I: Into<Self>,
    {
        if self.is_empty() { value.into() } else { self }
    }

    /// Returns a new `AvatarInformation` with the given value if the text is empty.
    /// Provided value is taken as is, not trimmed nor manipulated in any way, use with causation.
    /// Ideal input for this function would be a string that is one grapheme long.
    ///
    pub(crate) fn or_else_unchecked<S: AsRef<str>>(self, value: S) -> Self {
        if self.is_empty() {
            let name = value.as_ref();
            let text = name.to_string();
            let color = proton_color(name);

            Self {
                text,
                color: color.to_string(),
            }
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

        let text = first_emoji_grapheme(name)
            .or_else(|| name.unicode_words().find_map(first_grapheme_upppercase))
            .unwrap_or_default();

        let color = proton_color(name);

        Self {
            text,
            color: color.to_string(),
        }
    }
}

fn first_emoji_grapheme(s: &str) -> Option<String> {
    s.trim()
        .graphemes(true)
        .find(|g| {
            g.chars().next().is_some_and(|c| {
                ('\u{1F300}'..='\u{1FAFF}').contains(&c)        // Misc emoji
                    || ('\u{1F600}'..='\u{1F64F}').contains(&c) // Emoticons
                    || ('\u{2700}'..='\u{27BF}').contains(&c) // Dingbats & symbols
            })
        })
        .map(str::to_string)
}
