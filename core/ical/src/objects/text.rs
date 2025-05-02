use super::*;

/// Text.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.3.11>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text(String);

impl Text {
    /// Creates a text; illegal characters get reasonably sanitized.
    ///
    /// See also: [`Self::new_checked()`].
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = text
            .into()
            .chars()
            .filter_map(|ch| match ch {
                '\t' => Some(' '),
                '\n' => Some('\n'),
                ch if ch.is_control() => None,
                ch => Some(ch),
            })
            .collect();

        Self(text)
    }

    /// Creates a text; returns an error if given string contains illegal
    /// charaters.
    ///
    /// See also: [`Self::new()`].
    pub fn new_checked(text: impl Into<String>) -> Result<Self, TextViolation> {
        let text = text
            .into()
            .char_indices()
            .map(|(idx, ch)| {
                if ch == '\t' || ch.is_control() && ch != '\n' {
                    Err(TextViolation::IllegalCharacter(idx, ch))
                } else {
                    Ok(ch)
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(Self(text))
    }

    /// Creates a text without validating given string.
    ///
    /// This is useful mostly for constructing strings known to be correct at
    /// compile time, like `"SOME_CONST"` etc.
    ///
    /// Analyze [`Self::new_checked()`] for invariants you're expected to
    /// uphold.
    #[must_use]
    pub fn new_unchecked(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Returns the underlying string, decoded (i.e. it has literal newlines
    /// instead of the escaped `"\n"` sequence etc.).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the underlying string, decoded (i.e. it has literal newlines
    /// instead of the escaped `"\n"` sequence etc.).
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl<T> From<T> for Text
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Text reference - like [`Text`], but borrowed.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRef<'a>(&'a str);

impl<'a> TextRef<'a> {
    /// See [`Text::new_unchecked()`].
    #[must_use]
    pub fn new_unchecked(text: &'a str) -> Self {
        Self(text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TextViolation {
    #[error("illegal character 0x{:04x} at byte {0}", *.1 as u32)]
    IllegalCharacter(usize, char),
}
