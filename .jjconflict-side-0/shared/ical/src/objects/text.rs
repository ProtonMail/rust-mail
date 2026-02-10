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

impl IcsRead<Value> for Text {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut text = String::new();

        while let Some(ch) = r.char() {
            match ch {
                '\n' => {
                    break;
                }

                '\\' => {
                    let Some(ch) = r.value::<Spanned<_>>() else {
                        break;
                    };

                    match ch.value {
                        '\\' | ';' | ',' => {
                            text.push(ch.value);
                        }
                        'n' => {
                            text.push('\n');
                        }
                        '\n' => {
                            r.warn(ch.span, "unexpected newline character");
                            break;
                        }
                        _ => {
                            r.error(ch.span, "unrecognized escape sequence");
                        }
                    }
                }

                '"' => {
                    if r.hints().inside_quote {
                        break;
                    }

                    text.push(ch);
                }

                ch => {
                    text.push(ch);
                }
            }
        }

        Some(Self(text))
    }
}

impl IcsWrite<Value> for Text {
    fn write(&self, w: &mut IcsWriter) {
        w.value(TextRef(&self.0));
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Text {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            Some(Self::new(zval.str()?))
        }
    }

    impl IntoPhpZval for Text {
        const TYPE: PhpDataType = PhpDataType::String;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            zval.set_string(&self.0, persistent)
        }
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

impl IcsWrite<Value> for TextRef<'_> {
    fn write(&self, w: &mut IcsWriter) {
        for ch in self.0.chars() {
            match ch {
                '\\' => w.raw("\\\\"),
                ';' => w.raw("\\;"),
                ',' => w.raw("\\,"),
                '\n' => w.raw("\\n"),
                ch => w.raw(format_args!("{ch}")),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TextViolation {
    #[error("illegal character 0x{:04x} at byte {0}", *.1 as u32)]
    IllegalCharacter(usize, char),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[derive(Debug)]
    struct TestCase {
        given: &'static str,
        expected_new: &'static str,
        expected_new_checked: Result<&'static str, TextViolation>,
    }

    const TEST_SMOKE: TestCase = TestCase {
        given: "Hello World!",
        expected_new: "Hello World!",
        expected_new_checked: Ok("Hello World!"),
    };

    const TEST_TAB: TestCase = TestCase {
        given: "hello\tworld",
        expected_new: "hello world",
        expected_new_checked: Err(TextViolation::IllegalCharacter(5, '\t')),
    };

    const TEST_NULL: TestCase = TestCase {
        given: "hello \0 world",
        expected_new: "hello  world",
        expected_new_checked: Err(TextViolation::IllegalCharacter(6, '\0')),
    };

    const TEST_NEWLINE_1: TestCase = TestCase {
        given: "hello\nworld",
        expected_new: "hello\\nworld",
        expected_new_checked: Ok("hello\\nworld"),
    };

    const TEST_NEWLINE_2: TestCase = TestCase {
        given: "hello\r\nworld",
        expected_new: "hello\\nworld",
        expected_new_checked: Err(TextViolation::IllegalCharacter(5, '\r')),
    };

    const TEST_BACKSLASH: TestCase = TestCase {
        given: "hello \\ world",
        expected_new: "hello \\\\ world",
        expected_new_checked: Ok("hello \\\\ world"),
    };

    const TEST_SEMICOLON: TestCase = TestCase {
        given: "hello ; world",
        expected_new: "hello \\; world",
        expected_new_checked: Ok("hello \\; world"),
    };

    const TEST_COMMA: TestCase = TestCase {
        given: "hello , world",
        expected_new: "hello \\, world",
        expected_new_checked: Ok("hello \\, world"),
    };

    #[test_case(TEST_SMOKE)]
    #[test_case(TEST_TAB)]
    #[test_case(TEST_NULL)]
    #[test_case(TEST_NEWLINE_1)]
    #[test_case(TEST_NEWLINE_2)]
    #[test_case(TEST_BACKSLASH)]
    #[test_case(TEST_SEMICOLON)]
    #[test_case(TEST_COMMA)]
    fn test(case: TestCase) {
        let target = Text::new(case.given);

        assert_eq!(case.expected_new, target.to_string(Value));
        assert_trip!(case.expected_new, Text as Value);

        // ---

        let target = Text::new_checked(case.given)
            .map(|txt| txt.to_string(Value))
            .map_err(|err| err.to_string());

        assert_eq!(
            case.expected_new_checked
                .map(ToString::to_string)
                .map_err(|err| err.to_string()),
            target,
        );
    }
}
