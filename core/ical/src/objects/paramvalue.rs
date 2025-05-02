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

impl Read<Value> for ParamValue {
    fn read(r: &mut Reader) -> Option<Self> {
        let mut value = String::new();
        let quoted;

        if r.try_eat('"').is_some() {
            quoted = true;

            while let Some(ch) = r.char() {
                if ch == '"' || ch.is_control() {
                    break;
                }

                value.push(ch);
            }
        } else {
            quoted = false;

            while let Some(ch) = r.peek() {
                if ch == ';' || ch == ':' || ch == ',' || ch == '"' || ch.is_control() {
                    break;
                }

                value.push(r.char()?);
            }
        }

        Some(Self { value, quoted })
    }
}

impl Write<Value> for ParamValue {
    fn write(&self, w: &mut Writer) {
        if self.quoted {
            w.raw("\"");
        }

        w.raw(self.value.as_str());

        if self.quoted {
            w.raw("\"");
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for ParamValue {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            Some(Self::new(zval.str()?))
        }
    }

    impl IntoPhpZval for ParamValue {
        const TYPE: PhpDataType = PhpDataType::String;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            zval.set_string(&self.value, persistent)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParamValueViolation {
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
        expected_new_checked: Result<&'static str, ParamValueViolation>,
    }

    const TEST_SMOKE: TestCase = TestCase {
        given: "Hello World!",
        expected_new: "Hello World!",
        expected_new_checked: Ok("Hello World!"),
    };

    const TEST_SEMICOLON: TestCase = TestCase {
        given: "Hello ; World!",
        expected_new: "\"Hello ; World!\"",
        expected_new_checked: Ok("\"Hello ; World!\""),
    };

    const TEST_COLON: TestCase = TestCase {
        given: "Hello : World!",
        expected_new: "\"Hello : World!\"",
        expected_new_checked: Ok("\"Hello : World!\""),
    };

    const TEST_COMMA: TestCase = TestCase {
        given: "Hello , World!",
        expected_new: "\"Hello , World!\"",
        expected_new_checked: Ok("\"Hello , World!\""),
    };

    const TEST_CONTROL: TestCase = TestCase {
        given: "Hello \n\0 World!",
        expected_new: "Hello  World!",
        expected_new_checked: Err(ParamValueViolation::IllegalCharacter(6, '\n')),
    };

    const TEST_QUOTE: TestCase = TestCase {
        given: "Hello \" World!",
        expected_new: "Hello  World!",
        expected_new_checked: Err(ParamValueViolation::IllegalCharacter(6, '"')),
    };

    #[test_case(TEST_SMOKE)]
    #[test_case(TEST_SEMICOLON)]
    #[test_case(TEST_COLON)]
    #[test_case(TEST_COMMA)]
    #[test_case(TEST_CONTROL)]
    #[test_case(TEST_QUOTE)]
    fn test(case: TestCase) {
        let target = ParamValue::new(case.given);

        assert_eq!(case.expected_new, target.to_string(Value));
        assert_trip!(case.expected_new, ParamValue as Value);

        // ---

        let target = ParamValue::new_checked(case.given)
            .map(|txt| txt.to_string(Value))
            .map_err(|err| err.to_string());

        assert_eq!(
            case.expected_new_checked
                .map(ToString::to_string)
                .map_err(|err| err.to_string()),
            target,
        );
    }

    #[test]
    fn extra_quotes() {
        let actual = ParamValue::from_str("\"John Smith\"", Value)
            .unwrap()
            .into_string();

        assert_eq!("John Smith", actual);
    }
}
