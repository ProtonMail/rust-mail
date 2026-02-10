use regex::Regex;

/// Representation of a text value from vCard RFC6350
#[derive(Debug, Default, Clone, Eq, Hash, PartialEq)]
pub struct Text {
    pub value: String,
}

impl Text {
    /// Create a new `Text` value from a str (no check are done)
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self {
            value: unescape(value),
        }
    }
}

impl<T: AsRef<str>> From<T> for Text {
    fn from(value: T) -> Self {
        Self::new(value.as_ref())
    }
}

fn unescape(value: &str) -> String {
    value
        .replace(r"\,", ",")
        .replace(r"\n", "\n")
        .replace(r"\\", r"\")
}

/// This is unused, might be useful for the future if we want to limit what texts the user can
/// create.
// I don't think that it makes sense to reject invalid texts when parsing, as we can still display them.
#[must_use]
fn _is_text_value(value: &str) -> bool {
    // text = *TEXT-CHAR
    // TEXT-CHAR = "\\" / "\," / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-5B / %x5D-7E
    //    ; Backslashes, commas, and newlines must be encoded.
    let re =
        Regex::new(r"^(\\\\|\\,|\\n|[ \t]|[^\x00-\x7F]|[\x21-\x2B]|[\x2D-\x5B]|[\x5D-\x7E])*$")
            .unwrap();
    re.is_match(value)
}

/// Validate that given `value` respect format for `text-list` values
/// Unused as well
fn _is_text_list_value(value: &str) -> bool {
    // text-list             = text             *("," text)
    // text = *TEXT-CHAR
    // TEXT-CHAR = "\\" / "\," / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-5B / %x5D-7E
    //    ; Backslashes, commas, and newlines must be encoded.

    super::check_list(value, _is_text_value, ',').is_some()
}

#[cfg(test)]
mod test {
    use crate::values::text::unescape;

    #[test]
    fn test_unescape() {
        let text = unescape("\\\\ \\, \\n \t 𝕯!+-[]~");
        assert_eq!(text, "\\ , \n \t 𝕯!+-[]~");
    }
}
