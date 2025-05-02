use super::*;

/// Parameter value.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamValue {
    value: String,
    quoted: bool,
}

impl ParamValue {
    /// Creates a parameter value; invalid characters get reasonably sanitized.
    ///
    /// See also: [`Self::new_checked()`].
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        let mut quoted = false;

        let value = value
            .into()
            .chars()
            .filter(|ch| {
                if let ';' | ':' | ',' = ch {
                    quoted = true;
                }

                !ch.is_control() && *ch != '"'
            })
            .collect();

        Self { value, quoted }
    }

    /// Creates a parameter value; returns an error if given string contains
    /// illegal characters.
    ///
    /// See also: [`Self::new()`].
    pub fn new_checked(value: impl Into<String>) -> Result<Self, ParamValueViolation> {
        let mut quoted = false;

        let value = value
            .into()
            .char_indices()
            .map(|(idx, ch)| {
                if let ';' | ':' | ',' = ch {
                    quoted = true;
                }

                if ch.is_control() || ch == '"' {
                    Err(ParamValueViolation::IllegalCharacter(idx, ch))
                } else {
                    Ok(ch)
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { value, quoted })
    }

    /// Creates a parameter value without validating given string.
    ///
    /// This is useful mostly for constructing strings known to be correct at
    /// compile time, like `"SOME_CONST"` etc.
    ///
    /// Analyze [`Self::new_checked()`] for invariants you're expected to
    /// uphold.
    #[must_use]
    pub fn new_unchecked(value: impl Into<String>, quoted: bool) -> Self {
        Self {
            value: value.into(),
            quoted,
        }
    }

    /// Returns the underlying string, decoded (i.e. without any extra quotes).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns the underlying string, decoded (i.e. without any extra quotes).
    #[must_use]
    pub fn into_string(self) -> String {
        self.value
    }
}

impl<T> From<T> for ParamValue
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParamValueViolation {
    #[error("illegal character 0x{:04x} at byte {0}", *.1 as u32)]
    IllegalCharacter(usize, char),
}
