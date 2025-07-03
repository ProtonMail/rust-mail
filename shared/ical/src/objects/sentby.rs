use super::*;

/// Sent by.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.18>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct SentBy {
    pub value: EmailAddress,
}

impl<T> From<T> for SentBy
where
    T: Into<EmailAddress>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Value> for SentBy {
    fn read(r: &mut IcsReader) -> Option<Self> {
        if r.try_eat('"').is_some() {
            r.hint(
                |h| {
                    // Cursed!
                    //
                    // Most of the time addresses are the last part of the
                    // property, like:
                    //
                    // ```text
                    // ORGANIZER;CN=Someone:mailto:someone@somewhere.com
                    //                     ^---------------------------^
                    // ```
                    //
                    // This makes them greedy, i.e. they consume everything up
                    // to the end of the line - but `SentBy` is special, it's
                    // (usually) contained by DQUOTEs:
                    //
                    // ```text
                    // ORGANIZER;SENT-BY="mailto:someone-else@somewhere.com":localhost
                    //          ^------------------------------------------^
                    // ```
                    //
                    // So if we were to naively run [`EmailAddress:read()`]
                    // here, it would read everything up to and including the
                    // `:localhost` part, which is waay too much.
                    //
                    // So let's communicate "hey we're already inside quotes,
                    // please stop at the nearest quote thanks".
                    //
                    // Alternatively this could be solved through the type
                    // system (passing a `<QUOTED: bool>` const flag around),
                    // but that's actually more awkward than having an extra
                    // runtime flag, because it doesn't compose well with
                    // ext-php-rs etc.
                    h.inside_quote = true;
                },
                |r| {
                    if let Some(value) = r.value() {
                        Some(Self { value })
                    } else {
                        // If we've failed to parse the email address, let's
                        // recover by eating everything up to (and including)
                        // the quote we're supposed to be contained within.
                        r.silently(|r| {
                            while let Some(ch) = r.char() {
                                if ch == '\n' || ch == '"' {
                                    break;
                                }
                            }
                        });

                        None
                    }
                },
            )
        } else {
            let Spanned { span, value } = r.spanned(ParamValue::read)?;

            r.warn(span, "quirky email address (should be enquoted)");

            Some(Self {
                value: EmailAddress::from(value.into_string()),
            })
        }
    }
}

impl IcsWrite<Value> for SentBy {
    fn write(&self, w: &mut IcsWriter) {
        w.raw("\"");
        w.value(&self.value);
        w.raw("\"");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("\"mailto:someone@localhost\"")]
    #[test_case("\"mailto:someone@somewhere.com\"")]
    fn smoke(s: &str) {
        assert_trip!(s, SentBy as Value);
    }

    #[test]
    fn without_quotes() {
        let (obj, msgs) = SentBy::from_str_ex("someone@localhost", Value);

        assert_eq!(
            Some(SentBy {
                value: EmailAddress::from("someone@localhost")
            }),
            obj,
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 1), (1, 17))),
                body: "quirky email address (should be enquoted)".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs
        );
    }
}
