use super::*;

/// Parameter value.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamValue {
    value: String,
    quote: bool,
}

impl ParamValue {
    /// Creates a parameter value; invalid characters get reasonably sanitized.
    ///
    /// See also: [`Self::new_checked()`].
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        let mut quote = false;

        let value = value
            .into()
            .chars()
            .filter(|ch| {
                if let ';' | ':' | ',' = ch {
                    quote = true;
                }

                !ch.is_control() && *ch != '"'
            })
            .collect();

        Self { value, quote }
    }

    /// Creates a parameter value; returns an error if given string contains
    /// illegal characters.
    ///
    /// See also: [`Self::new()`].
    pub fn new_checked(value: impl Into<String>) -> Result<Self, ParamValueViolation> {
        let mut quote = false;

        let value = value
            .into()
            .char_indices()
            .map(|(idx, ch)| {
                if let ';' | ':' | ',' = ch {
                    quote = true;
                }

                if ch.is_control() || ch == '"' {
                    Err(ParamValueViolation::IllegalCharacter(idx, ch))
                } else {
                    Ok(ch)
                }
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { value, quote })
    }

    /// Creates a parameter value without validating given string.
    ///
    /// This is useful mostly for constructing strings known to be correct at
    /// compile time, like `"SOME_CONST"` etc.
    ///
    /// Analyze [`Self::new_checked()`] for invariants you're expected to
    /// uphold.
    #[must_use]
    pub fn new_unchecked(value: impl Into<String>, quote: bool) -> Self {
        Self {
            value: value.into(),
            quote,
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

impl IcsRead<Value> for ParamValue {
    fn read(r: &mut IcsReader) -> Option<Self> {
        #[derive(Clone, Copy, Debug)]
        enum Mode {
            Quoted { quote_span: Span },
            Plain,
        }

        let mut value = String::new();
        let mut mode;
        let mut quote;

        if let Some(q) = r.spanned(|r| r.try_eat('"')) {
            mode = Mode::Quoted { quote_span: q.span };
            quote = true;
        } else {
            mode = Mode::Plain;
            quote = false;
        }

        while let Some(ch) = r.peek() {
            match (ch, mode) {
                ('"', Mode::Quoted { .. }) => {
                    _ = r.char();
                    break;
                }

                (';' | ':' | '"', Mode::Plain) => {
                    break;
                }

                (',', Mode::Plain) => {
                    if r.hints().inside_array {
                        break;
                    }

                    r.warn(Span::one(r.pos()), "quirky comma");
                    value.push(r.char()?);

                    // Even though we can read this comma, it's not supposed to
                    // be here - so when printing back, enquote the string for
                    // better compatibility
                    quote = true;
                }

                ('\\', Mode::Plain) => {
                    _ = r.char();

                    let span = Span::new(r.pos().prev(), r.pos());

                    match r.char()? {
                        ch @ (';' | ':' | ',') => {
                            r.warn(span, "quirky escape sequence");
                            value.push(ch);

                            // Even though we can read this escape, it's not
                            // supposed to be here - so when printing back,
                            // enquote the string for better compatibility
                            quote = true;
                        }

                        _ => {
                            r.error(span, "unrecognized escape sequence");
                        }
                    }
                }

                ('\\', Mode::Quoted { quote_span }) => {
                    // Usually strings are either unquoted:
                    //
                    // ```
                    // ORGANIZER;CN=Jon:mailto:jon.snow@localhost
                    //              ^-^
                    // ```
                    //
                    // ... or quoted from both sides:
                    //
                    // ```
                    // ORGANIZER;CN="Jon Snow":mailto:jon.snow@localhost
                    //              ^--------^
                    // ```
                    //
                    // But if the original string happens to *begin* with a
                    // quote, as in:
                    //
                    // ```
                    // let organizer = r#""Jon Snow", Ostéopathe"#;
                    // ```
                    //
                    // ... some clients will only escape its right-hand side:
                    //
                    // ```
                    // ORGANIZER;CN="Jon Snow\", Ostéopathe:mailto:jon.snow@localhost
                    //              ^------- !! ----------^
                    // ```
                    //
                    // We discover this by stumbling upon the `\"` sigil - this
                    // means that the first `"` we saw was a literal quote char,
                    // not the beginning of a quoted-string we thought it was.
                    //
                    // This also means we have to switch the parsing mode, since
                    // we are not - in fact - inside a quoted string.
                    //
                    // (which matters, because quoted strings end on `"`, which
                    // is not the case here - we must parse up to `:`, as if
                    // this was a plain string.)
                    //
                    // Note that even though we know the string is supposed to
                    // contain quotes, we remove them - we will parse this
                    // string as:
                    //
                    // ```
                    // let organizer = r#"Jon Snow, Ostéopathe"#;
                    // ```
                    //
                    // That's because even though we can read quotes, they are
                    // not supported by the standard, so there's no way for us
                    // to reliably print those back; they just shouldn't be
                    // here whatsoever.
                    if r.try_string("\\\"").is_some() {
                        r.warn(quote_span, "quirky quote");

                        mode = Mode::Plain;
                        quote = false;
                    } else {
                        _ = r.char();

                        let span = Span::new(r.pos().prev(), r.pos());

                        match r.char()? {
                            ch @ (';' | ':' | ',') => {
                                r.warn(span, "quirky escape sequence");
                                value.push(ch);
                            }
                            _ => {
                                r.error(span, "unrecognized escape sequence");
                            }
                        }
                    }
                }

                ('\t', _) => {
                    r.warn(Span::one(r.pos()), "quirky tab");
                    _ = r.char();

                    // param-values don't support tabs even in quotes, so let's
                    // sanitize it into a space for better compatibility
                    value.push(' ');
                }

                (ch, _) if ch.is_control() => {
                    break;
                }

                _ => {
                    value.push(r.char()?);
                }
            }
        }

        Some(Self { value, quote })
    }
}

impl IcsWrite<Value> for ParamValue {
    fn write(&self, w: &mut IcsWriter) {
        if self.quote {
            w.raw("\"");
        }

        w.raw(self.value.as_str());

        if self.quote {
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
        const NULLABLE: bool = false;

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

    #[test_case(';' ; "semicolon")]
    #[test_case(':' ; "colon")]
    #[test_case(',' ; "comma")]
    fn plain_escape(ch: char) {
        let (obj, msgs) = ParamValue::from_str_ex(&format!("John Smith\\{ch} MD"), Value);

        assert_eq!(
            Some(ParamValue {
                value: format!("John Smith{ch} MD"),
                quote: true,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 11), (1, 12))),
                body: "quirky escape sequence".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs,
        );
    }

    #[test]
    fn unrecognized_plain_escape() {
        let (obj, msgs) = ParamValue::from_str_ex("John Smith\\n MD", Value);

        assert_eq!(
            Some(ParamValue {
                value: "John Smith MD".into(),
                quote: false,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 11), (1, 12))),
                body: "unrecognized escape sequence".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }

    #[test_case(';' ; "semicolon")]
    #[test_case(':' ; "colon")]
    #[test_case(',' ; "comma")]
    fn quoted_escape(ch: char) {
        let (obj, msgs) = ParamValue::from_str_ex(&format!("\"John Smith\\{ch} MD\""), Value);

        assert_eq!(
            Some(ParamValue {
                value: format!("John Smith{ch} MD"),
                quote: true,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 12), (1, 13))),
                body: "quirky escape sequence".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs,
        );
    }

    #[test]
    fn unrecognized_quoted_escape() {
        let (obj, msgs) = ParamValue::from_str_ex("\"John Smith\\n MD\"", Value);

        assert_eq!(
            Some(ParamValue {
                value: "John Smith MD".into(),
                quote: true,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 12), (1, 13))),
                body: "unrecognized escape sequence".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }

    #[test]
    fn escape_trip() {
        let actual = ParamValue::from_str_ex("John Smith\\, MD", Value)
            .0
            .unwrap()
            .to_string(Value);

        assert_eq!("\"John Smith, MD\"", actual);
    }

    #[test]
    fn quirky_comma() {
        let (obj, msgs) = ParamValue::from_str_ex("Gregory House, MD", Value);

        assert_eq!(
            Some(ParamValue {
                value: "Gregory House, MD".into(),
                quote: true,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 14), (1, 14))),
                body: "quirky comma".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs,
        );

        // ---

        let actual = Vec::<ParamValue>::from_str("Gregory House, MD", Value).unwrap();

        let expected = vec![
            ParamValue {
                value: "Gregory House".into(),
                quote: false,
            },
            ParamValue {
                value: " MD".into(),
                quote: false,
            },
        ];

        assert_eq!(expected, actual);
    }

    #[test]
    fn quirky_tab() {
        let (obj, msgs) = ParamValue::from_str_ex("Grzegorz\tBrzęczyszczykiewicz", Value);

        assert_eq!(
            Some(ParamValue {
                value: "Grzegorz Brzęczyszczykiewicz".into(),
                quote: false,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 9), (1, 9))),
                body: "quirky tab".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs,
        );
    }

    #[test]
    fn quirky_quote() {
        let (obj, msgs) = ParamValue::from_str_ex("\"John Smith\\\"", Value);

        assert_eq!(
            Some(ParamValue {
                value: "John Smith".into(),
                quote: false,
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 1), (1, 1))),
                body: "quirky quote".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            },],
            msgs,
        );
    }

    #[test]
    fn quirky_quote_and_quirky_comma() {
        let (obj, msgs) = ParamValue::from_str_ex("\"John Smith\\\", M.D.", Value);

        assert_eq!(
            Some(ParamValue {
                value: "John Smith, M.D.".into(),
                quote: true,
            }),
            obj,
        );

        assert_eq!(
            vec![
                ReadMsg {
                    at: Some(Span::new((1, 1), (1, 1))),
                    body: "quirky quote".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
                ReadMsg {
                    at: Some(Span::new((1, 14), (1, 14))),
                    body: "quirky comma".into(),
                    kind: ReadMsgKind::Warning,
                    context: Vec::new(),
                },
            ],
            msgs,
        );
    }
}
